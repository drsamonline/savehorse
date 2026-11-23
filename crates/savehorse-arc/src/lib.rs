//! arc — archives.
//!
//! Ported from `arc` 1.0.0 (generation 2), which used the `zip` crate. Two
//! rough edges are smoothed: a directory entry whose name cannot be read no
//! longer aborts a listing, and a bad `--filter` pattern returns an error
//! instead of panicking inside a `Regex::new(...).unwrap()`.
//!
//! Extraction keeps the original's safety property: every entry passes through
//! `enclosed_name`, so an archive cannot write outside the destination.

use regex::Regex;
use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: String,
    pub crc32: String,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub archive: String,
    pub files_added: usize,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub compression_method: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    pub archive: String,
    pub output_dir: String,
    pub extracted: Vec<String>,
    pub skipped: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    pub archive: String,
    pub entries: usize,
    pub passed: usize,
    pub failed: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SplitManifest {
    pub original_filename: String,
    pub total_size: u64,
    pub chunk_size: u64,
    pub chunk_count: usize,
    pub chunks: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct JoinReport {
    pub output: String,
    pub chunks: usize,
    pub total_bytes: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn zip_err(context: &str, e: zip::result::ZipError) -> Error {
    Error::Invalid(format!("{}: {}", context, e))
}

fn io_err(context: &str, e: io::Error) -> Error {
    Error::Invalid(format!("{}: {}", context, e))
}

/// Every file and directory under `dir`, paired with the name it will carry
/// inside the archive. Directories get a trailing slash, as the format expects.
fn collect_paths(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, String)>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    // Sorted before recursing. The filesystem hands back entries in whatever
    // order it likes, so the same directory produced different archives on
    // different runs and two builds of the same input never matched.
    let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    children.sort();

    for path in children {
        let rel = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if path.is_dir() {
            out.push((path.clone(), format!("{}/", rel)));
            collect_paths(&path, base, out);
        } else if path.is_file() {
            out.push((path, rel));
        }
    }
}

fn ratio(uncompressed: u64, compressed: u64) -> String {
    if uncompressed > 0 {
        format!(
            "{:.1}%",
            (1.0 - (compressed as f64 / uncompressed as f64)) * 100.0
        )
    } else {
        "0.0%".to_string()
    }
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Pack files and directories into a zip archive.
pub fn create_archive(archive_path: &Path, paths: &[PathBuf], store: bool) -> Result<CreateReport> {
    if paths.is_empty() {
        return Err(Error::Invalid("nothing to archive".to_string()));
    }
    if let Some(parent) = archive_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(archive_path)?;
    let mut zip = ZipWriter::new(file);

    let method = if store {
        CompressionMethod::Stored
    } else {
        CompressionMethod::Deflated
    };
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let mut files_added = 0usize;
    let mut bytes_in = 0u64;

    for path in paths {
        if path.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| "file".to_string());

            zip.start_file(name, options)
                .map_err(|e| zip_err("could not start entry", e))?;
            let mut f = File::open(path)?;
            bytes_in += io::copy(&mut f, &mut zip).map_err(|e| io_err("write failed", e))?;
            files_added += 1;
        } else if path.is_dir() {
            let mut list = Vec::new();
            let base = path.parent().unwrap_or(path);
            collect_paths(path, base, &mut list);

            for (source, name) in list {
                if source.is_dir() {
                    zip.add_directory(name, options)
                        .map_err(|e| zip_err("could not add directory", e))?;
                } else {
                    zip.start_file(name, options)
                        .map_err(|e| zip_err("could not start entry", e))?;
                    let mut f = File::open(&source)?;
                    bytes_in += io::copy(&mut f, &mut zip).map_err(|e| io_err("write failed", e))?;
                    files_added += 1;
                }
            }
        }
    }

    zip.finish().map_err(|e| zip_err("could not finish archive", e))?;

    Ok(CreateReport {
        archive: archive_path.to_string_lossy().to_string(),
        files_added,
        bytes_in,
        bytes_out: fs::metadata(archive_path)?.len(),
        compression_method: if store { "stored" } else { "deflate" }.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// List every entry with its sizes and CRC.
pub fn list_archive(archive_path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(archive_path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| zip_err("not a readable zip archive", e))?;

    let mut entries = Vec::with_capacity(archive.len());

    for i in 0..archive.len() {
        let entry = match archive.by_index(i) {
            Ok(e) => e,
            // A damaged entry should not stop the whole listing.
            Err(_) => continue,
        };

        let name = match entry.name() {
            Ok(n) => n.to_string(),
            Err(_) => format!("<unreadable entry {}>", i),
        };
        let uncompressed = entry.size();
        let compressed = entry.compressed_size();

        entries.push(ArchiveEntry {
            name,
            uncompressed_size: uncompressed,
            compressed_size: compressed,
            compression_ratio: ratio(uncompressed, compressed),
            crc32: format!("{:08x}", entry.crc32()),
            is_dir: entry.is_dir(),
        });
    }

    Ok(entries)
}

/// Extract entries, optionally only those whose name matches `filter`.
pub fn extract_archive(
    archive_path: &Path,
    out_dir: &Path,
    filter: Option<&str>,
) -> Result<ExtractReport> {
    let filter_re = match filter {
        Some(f) if !f.trim().is_empty() => Some(
            Regex::new(f).map_err(|e| Error::Invalid(format!("invalid filter pattern: {}", e)))?,
        ),
        _ => None,
    };

    let file = File::open(archive_path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| zip_err("not a readable zip archive", e))?;

    fs::create_dir_all(out_dir)?;

    let mut extracted = Vec::new();
    let mut skipped = 0usize;

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        let name = match entry.name() {
            Ok(n) => n.to_string(),
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        if let Some(ref re) = filter_re {
            if !re.is_match(&name) {
                continue;
            }
        }

        // Never trust an archive's own idea of a path.
        let safe = match entry.enclosed_name() {
            Some(p) => p,
            None => {
                skipped += 1;
                continue;
            }
        };
        let destination = out_dir.join(safe);

        if entry.is_dir() {
            fs::create_dir_all(&destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            let mut out = File::create(&destination)?;
            io::copy(&mut entry, &mut out)?;
            extracted.push(destination.to_string_lossy().to_string());
        }
    }

    Ok(ExtractReport {
        archive: archive_path.to_string_lossy().to_string(),
        output_dir: out_dir.to_string_lossy().to_string(),
        extracted,
        skipped,
    })
}

/// Read every entry to the end, which is what actually checks the CRC.
pub fn test_archive(archive_path: &Path) -> Result<TestReport> {
    let file = File::open(archive_path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| zip_err("not a readable zip archive", e))?;

    let entries = archive.len();
    let mut passed = 0usize;
    let mut failed = Vec::new();
    let mut buffer = [0u8; 65536];

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                failed.push(format!("entry {}: {}", i, e));
                continue;
            }
        };

        let name = match entry.name() {
            Ok(n) => n.to_string(),
            Err(_) => format!("entry {}", i),
        };

        let mut broken = None;
        loop {
            match entry.read(&mut buffer) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(e) => {
                    broken = Some(e.to_string());
                    break;
                }
            }
        }

        match broken {
            Some(why) => failed.push(format!("{}: {}", name, why)),
            None => passed += 1,
        }
    }

    Ok(TestReport {
        archive: archive_path.to_string_lossy().to_string(),
        entries,
        passed,
        failed,
    })
}

// ---------------------------------------------------------------------------
// Split and join
// ---------------------------------------------------------------------------

/// Cut a file into numbered chunks and write a manifest beside them.
pub fn split_file(file_path: &Path, chunk_size: u64, out_dir: &Path) -> Result<SplitManifest> {
    if chunk_size == 0 {
        return Err(Error::Invalid("chunk size must be greater than zero".to_string()));
    }
    if !file_path.is_file() {
        return Err(Error::NotFound(file_path.display().to_string()));
    }

    let mut source = BufReader::new(File::open(file_path)?);
    let total_size = fs::metadata(file_path)?.len();
    let filename = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());

    fs::create_dir_all(out_dir)?;

    let mut chunks = Vec::new();
    let mut index = 1usize;
    let mut remaining = total_size;
    let buffer_size = 65536u64.min(chunk_size) as usize;
    let mut buffer = vec![0u8; buffer_size.max(1)];

    while remaining > 0 {
        let chunk_name = format!("{}.{:03}", filename, index);
        let mut chunk = BufWriter::new(File::create(out_dir.join(&chunk_name))?);

        let mut to_write = chunk_size.min(remaining);
        while to_write > 0 {
            let take = (buffer.len() as u64).min(to_write) as usize;
            source.read_exact(&mut buffer[..take])?;
            chunk.write_all(&buffer[..take])?;
            to_write -= take as u64;
            remaining -= take as u64;
        }

        chunk.flush()?;
        chunks.push(chunk_name);
        index += 1;
    }

    let manifest = SplitManifest {
        original_filename: filename.clone(),
        total_size,
        chunk_size,
        chunk_count: chunks.len(),
        chunks,
    };

    let manifest_path = out_dir.join(format!("{}.manifest.json", filename));
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())?;

    Ok(manifest)
}

/// Reassemble chunks, either from a manifest or from the `.001` naming.
pub fn join_chunks(input: &Path, output: Option<&Path>) -> Result<JoinReport> {
    let mut chunk_paths: Vec<PathBuf> = Vec::new();
    let mut destination: Option<PathBuf> = output.map(|p| p.to_path_buf());

    let is_manifest = input
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    if is_manifest {
        let text = fs::read_to_string(input)?;
        let manifest: SplitManifest = serde_json::from_str(&text)
            .map_err(|e| Error::Invalid(format!("not a split manifest: {}", e)))?;
        let dir = input.parent().unwrap_or(Path::new("."));
        for name in &manifest.chunks {
            chunk_paths.push(dir.join(name));
        }
        if destination.is_none() {
            destination = Some(dir.join(&manifest.original_filename));
        }
    } else {
        let filename = input
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let base = match filename.rfind(".001") {
            Some(idx) => filename[..idx].to_string(),
            None => filename.clone(),
        };
        let dir = input.parent().unwrap_or(Path::new("."));

        let mut index = 1;
        loop {
            let candidate = dir.join(format!("{}.{:03}", base, index));
            if candidate.exists() {
                chunk_paths.push(candidate);
                index += 1;
            } else {
                break;
            }
        }
        if destination.is_none() {
            destination = Some(dir.join(&base));
        }
    }

    if chunk_paths.is_empty() {
        return Err(Error::Invalid("no chunks found to join".to_string()));
    }

    let dest = destination.unwrap();
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut out = BufWriter::new(File::create(&dest)?);
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];

    for chunk in &chunk_paths {
        let mut f = BufReader::new(File::open(chunk)?);
        loop {
            let n = f.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            out.write_all(&buffer[..n])?;
            total += n as u64;
        }
    }
    out.flush()?;

    Ok(JoinReport {
        output: dest.to_string_lossy().to_string(),
        chunks: chunk_paths.len(),
        total_bytes: total,
    })
}


// ---------------------------------------------------------------------------
// Transfer planning, torrents and probes (from vortex)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SegmentPlan {
    pub segment_index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub length_bytes: u64,
    pub range_header: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TorrentFileItem {
    pub path: String,
    pub length: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TorrentMetadata {
    pub announce: Option<String>,
    pub name: String,
    pub piece_length: u64,
    pub piece_count: usize,
    pub total_bytes: u64,
    pub files: Vec<TorrentFileItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ProbeResult {
    pub host: String,
    pub port: u16,
    pub connected: bool,
    pub latency_ms: u64,
    pub detail: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UpnpPayloads {
    pub ssdp_discovery: String,
    pub soap_add_port_mapping: String,
}

/// Split a size into byte ranges, ready to be sent as HTTP Range headers.
pub fn plan_segments(total_size: u64, segments: usize) -> Vec<SegmentPlan> {
    if segments == 0 || total_size == 0 {
        return Vec::new();
    }

    let count = segments as u64;
    let base = total_size / count;
    let mut plans = Vec::with_capacity(segments);
    let mut start = 0u64;

    for index in 0..segments {
        let is_last = index == segments - 1;
        let end = if is_last {
            total_size - 1
        } else {
            (start + base - 1).min(total_size - 1)
        };
        let length = end - start + 1;

        plans.push(SegmentPlan {
            segment_index: index,
            start_byte: start,
            end_byte: end,
            length_bytes: length,
            range_header: format!("bytes={}-{}", start, end),
        });

        start = end + 1;
        if start >= total_size {
            break;
        }
    }

    plans
}

// --- bencode ---------------------------------------------------------------

enum Bencode {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Bencode>),
    Dict(Vec<(String, Bencode)>),
}

impl Bencode {
    fn get(&self, key: &str) -> Option<&Bencode> {
        match self {
            Bencode::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            Bencode::Int(v) => Some(*v),
            _ => None,
        }
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Bencode::Bytes(b) => Some(b),
            _ => None,
        }
    }

    fn as_text(&self) -> Option<String> {
        self.as_bytes().map(|b| String::from_utf8_lossy(b).to_string())
    }
}

fn parse_bencode(data: &[u8], pos: &mut usize) -> Result<Bencode> {
    if *pos >= data.len() {
        return Err(Error::Invalid("bencode: unexpected end of data".to_string()));
    }

    match data[*pos] {
        b'i' => {
            *pos += 1;
            let start = *pos;
            while *pos < data.len() && data[*pos] != b'e' {
                *pos += 1;
            }
            let text = std::str::from_utf8(&data[start..*pos])
                .map_err(|_| Error::Invalid("bencode: bad integer".to_string()))?;
            let value = text
                .parse::<i64>()
                .map_err(|_| Error::Invalid("bencode: bad integer".to_string()))?;
            *pos += 1;
            Ok(Bencode::Int(value))
        }
        b'l' => {
            *pos += 1;
            let mut items = Vec::new();
            while *pos < data.len() && data[*pos] != b'e' {
                items.push(parse_bencode(data, pos)?);
            }
            *pos += 1;
            Ok(Bencode::List(items))
        }
        b'd' => {
            *pos += 1;
            let mut entries = Vec::new();
            while *pos < data.len() && data[*pos] != b'e' {
                let key = match parse_bencode(data, pos)? {
                    Bencode::Bytes(b) => String::from_utf8_lossy(&b).to_string(),
                    _ => return Err(Error::Invalid("bencode: key is not a string".to_string())),
                };
                let value = parse_bencode(data, pos)?;
                entries.push((key, value));
            }
            *pos += 1;
            Ok(Bencode::Dict(entries))
        }
        b'0'..=b'9' => {
            let start = *pos;
            while *pos < data.len() && data[*pos] != b':' {
                *pos += 1;
            }
            let length_text = std::str::from_utf8(&data[start..*pos])
                .map_err(|_| Error::Invalid("bencode: bad length".to_string()))?;
            let length = length_text
                .parse::<usize>()
                .map_err(|_| Error::Invalid("bencode: bad length".to_string()))?;
            *pos += 1;
            if *pos + length > data.len() {
                return Err(Error::Invalid("bencode: string runs past the end".to_string()));
            }
            let bytes = data[*pos..*pos + length].to_vec();
            *pos += length;
            Ok(Bencode::Bytes(bytes))
        }
        other => Err(Error::Invalid(format!(
            "bencode: unexpected byte 0x{:02X}",
            other
        ))),
    }
}

/// Read a `.torrent` file: name, piece size, total length and the file list.
///
/// This reads the metadata; it does not download anything and does not speak to
/// a tracker.
pub fn parse_torrent(path: &Path) -> Result<TorrentMetadata> {
    let data = fs::read(path)?;
    let mut pos = 0usize;
    let root = parse_bencode(&data, &mut pos)?;

    let announce = root.get("announce").and_then(|v| v.as_text());
    let info = root
        .get("info")
        .ok_or_else(|| Error::Invalid("not a torrent: no info dictionary".to_string()))?;

    let name = info.get("name").and_then(|v| v.as_text()).unwrap_or_default();
    let piece_length = info
        .get("piece length")
        .and_then(|v| v.as_int())
        .unwrap_or(0) as u64;

    let piece_count = info
        .get("pieces")
        .and_then(|v| v.as_bytes())
        .map(|p| p.len() / 20)
        .unwrap_or(0);

    let mut files = Vec::new();
    let mut total = 0u64;

    if let Some(Bencode::List(entries)) = info.get("files") {
        for entry in entries {
            let length = entry.get("length").and_then(|v| v.as_int()).unwrap_or(0) as u64;
            let mut parts: Vec<String> = Vec::new();
            if let Some(Bencode::List(path_parts)) = entry.get("path") {
                for part in path_parts {
                    if let Some(text) = part.as_text() {
                        parts.push(text);
                    }
                }
            }
            total += length;
            files.push(TorrentFileItem {
                path: parts.join("/"),
                length,
            });
        }
    } else if let Some(length) = info.get("length").and_then(|v| v.as_int()) {
        total = length as u64;
        files.push(TorrentFileItem {
            path: name.clone(),
            length: total,
        });
    }

    Ok(TorrentMetadata {
        announce,
        name,
        piece_length,
        piece_count,
        total_bytes: total,
        files,
    })
}

/// Open a TCP connection and time it. Says nothing about what is behind it.
pub fn probe_tcp(host: &str, port: u16, timeout_ms: u64) -> Result<ProbeResult> {
    use std::net::{TcpStream, ToSocketAddrs};
    use std::time::{Duration, Instant};

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let address = format!("{}:{}", host, port);

    let mut resolved = match address.to_socket_addrs() {
        Ok(iter) => iter,
        Err(e) => {
            return Ok(ProbeResult {
                host: host.to_string(),
                port,
                connected: false,
                latency_ms: 0,
                detail: format!("could not resolve '{}': {}", address, e),
            })
        }
    };

    let target = match resolved.next() {
        Some(addr) => addr,
        None => {
            return Ok(ProbeResult {
                host: host.to_string(),
                port,
                connected: false,
                latency_ms: 0,
                detail: format!("'{}' resolved to no addresses", address),
            })
        }
    };

    let started = Instant::now();
    match TcpStream::connect_timeout(&target, timeout) {
        Ok(_) => Ok(ProbeResult {
            host: host.to_string(),
            port,
            connected: true,
            latency_ms: started.elapsed().as_millis() as u64,
            detail: format!("connected to {}", target),
        }),
        Err(e) => Ok(ProbeResult {
            host: host.to_string(),
            port,
            connected: false,
            latency_ms: started.elapsed().as_millis() as u64,
            detail: format!("{}", e),
        }),
    }
}

/// Build the two UPnP payloads a port mapping needs.
///
/// These are strings, not requests: nothing is sent, and no router is asked for
/// anything. Generating them is the part that is easy to get wrong; sending
/// them is the part that needs a network and a device that agrees.
pub fn upnp_payloads(external_port: u16, internal_port: u16, internal_ip: &str, protocol: &str) -> UpnpPayloads {
    let proto = match protocol.to_uppercase().as_str() {
        "TCP" => "TCP",
        _ => "UDP",
    };

    let ssdp_discovery = format!(
        "M-SEARCH * HTTP/1.1\r\n\
HOST: 239.255.255.250:1900\r\n\
MAN: \"ssdp:discover\"\r\n\
MX: 2\r\n\
ST: urn:schemas-upnp-org:service:WANIPConnection:1\r\n\
\r\n"
    );

    let soap_add_port_mapping = format!(
        "<?xml version=\"1.0\"?>\n\
<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\n\
  <s:Body>\n\
    <u:AddPortMapping xmlns:u=\"urn:schemas-upnp-org:service:WANIPConnection:1\">\n\
      <NewRemoteHost></NewRemoteHost>\n\
      <NewExternalPort>{}</NewExternalPort>\n\
      <NewProtocol>{}</NewProtocol>\n\
      <NewInternalPort>{}</NewInternalPort>\n\
      <NewInternalClient>{}</NewInternalClient>\n\
      <NewEnabled>1</NewEnabled>\n\
      <NewPortMappingDescription>savehorse</NewPortMappingDescription>\n\
      <NewLeaseDuration>0</NewLeaseDuration>\n\
    </u:AddPortMapping>\n\
  </s:Body>\n\
</s:Envelope>\n",
        external_port, proto, internal_port, internal_ip
    );

    UpnpPayloads {
        ssdp_discovery,
        soap_add_port_mapping,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-arc-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }


    #[test]
    fn archive_entries_are_in_a_stable_order() {
        // The same directory must produce the same archive twice, or checksums
        // are meaningless and two runs of the tool disagree.
        let dir = std::env::temp_dir().join(format!("savehorse-arc-order-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for name in ["c.txt", "a.txt", "b.txt"] {
            fs::write(dir.join(name), b"x").unwrap();
        }

        let mut first = Vec::new();
        collect_paths(&dir, &dir, &mut first);
        let mut second = Vec::new();
        collect_paths(&dir, &dir, &mut second);

        let names: Vec<&str> = first.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"], "entries are sorted");
        assert_eq!(first, second, "two walks of the same directory agree");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn packs_a_tree_and_reads_it_back() {
        let dir = scratch("roundtrip");
        let src = dir.join("tree");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("a.txt"), b"alpha").unwrap();
        fs::write(src.join("nested").join("b.txt"), b"beta").unwrap();

        let archive = dir.join("out.zip");
        let report = create_archive(&archive, &[src.clone()], false).unwrap();
        assert_eq!(report.files_added, 2);
        assert!(report.bytes_out > 0);

        let entries = list_archive(&archive).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "got {:?}", names);
        assert!(names.iter().any(|n| n.ends_with("b.txt")), "got {:?}", names);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extracts_and_preserves_contents() {
        let dir = scratch("extract");
        let src = dir.join("tree");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("hello.txt"), b"hello world").unwrap();

        let archive = dir.join("out.zip");
        create_archive(&archive, &[src.clone()], false).unwrap();

        let dest = dir.join("out");
        let report = extract_archive(&archive, &dest, None).unwrap();
        assert_eq!(report.extracted.len(), 1);
        assert_eq!(report.skipped, 0);

        let restored = fs::read(dest.join("tree").join("hello.txt")).unwrap();
        assert_eq!(restored, b"hello world");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_honours_a_filter_and_rejects_a_bad_one() {
        let dir = scratch("filter");
        let src = dir.join("tree");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("keep.txt"), b"k").unwrap();
        fs::write(src.join("skip.log"), b"s").unwrap();

        let archive = dir.join("out.zip");
        create_archive(&archive, &[src.clone()], false).unwrap();

        let dest = dir.join("out");
        let report = extract_archive(&archive, &dest, Some(r"\.txt$")).unwrap();
        assert_eq!(report.extracted.len(), 1);

        // A malformed pattern must be an error, not a panic.
        assert!(extract_archive(&archive, &dest, Some("([")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_mode_is_larger_but_valid() {
        let dir = scratch("store");
        let file = dir.join("data.txt");
        fs::write(&file, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();

        let deflated = dir.join("deflated.zip");
        let stored = dir.join("stored.zip");
        let a = create_archive(&deflated, &[file.clone()], false).unwrap();
        let b = create_archive(&stored, &[file.clone()], true).unwrap();

        assert_eq!(a.compression_method, "deflate");
        assert_eq!(b.compression_method, "stored");
        assert!(b.bytes_out >= a.bytes_out, "stored should not be smaller");
        assert_eq!(list_archive(&stored).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tests_a_healthy_archive() {
        let dir = scratch("test");
        let file = dir.join("a.txt");
        fs::write(&file, b"payload").unwrap();
        let archive = dir.join("a.zip");
        create_archive(&archive, &[file], false).unwrap();

        let report = test_archive(&archive).unwrap();
        assert_eq!(report.entries, 1);
        assert_eq!(report.passed, 1);
        assert!(report.failed.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn splitting_and_joining_round_trips() {
        let dir = scratch("split");
        let source = dir.join("big.bin");
        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&source, &payload).unwrap();

        let parts = dir.join("parts");
        let manifest = split_file(&source, 1024, &parts).unwrap();
        assert_eq!(manifest.chunk_count, 5);
        assert_eq!(manifest.total_size, 5000);

        // Join from the manifest.
        let rejoined = dir.join("rejoined.bin");
        let report = join_chunks(&parts.join("big.bin.manifest.json"), Some(&rejoined)).unwrap();
        assert_eq!(report.chunks, 5);
        assert_eq!(report.total_bytes, 5000);
        assert_eq!(fs::read(&rejoined).unwrap(), payload);

        // Join from the .001 naming, without a manifest.
        let auto = dir.join("auto.bin");
        join_chunks(&parts.join("big.bin.001"), Some(&auto)).unwrap();
        assert_eq!(fs::read(&auto).unwrap(), payload);
        let _ = fs::remove_dir_all(&dir);
    }


    #[test]
    fn segments_cover_the_whole_file() {
        let plans = plan_segments(1000, 4);
        assert_eq!(plans.len(), 4);
        assert_eq!(plans[0].start_byte, 0);
        assert_eq!(plans[0].range_header, "bytes=0-249");
        assert_eq!(plans[3].end_byte, 999, "the last segment takes the remainder");

        let covered: u64 = plans.iter().map(|p| p.length_bytes).sum();
        assert_eq!(covered, 1000, "no gaps, no overlap");

        assert!(plan_segments(0, 4).is_empty());
        assert!(plan_segments(100, 0).is_empty());
    }

    #[test]
    fn parses_a_torrent() {
        let dir = scratch("torrent");
        let file = dir.join("sample.torrent");

        // A minimal single-file torrent, hand-encoded.
        let mut body = Vec::new();
        body.extend_from_slice(b"d8:announce18:http://tracker/ann4:infod6:lengthi1024e4:name8:file.bin12:piece lengthi16384e6:pieces20:");
        body.extend_from_slice(&[0u8; 20]);
        body.extend_from_slice(b"ee");
        fs::write(&file, &body).unwrap();

        let meta = parse_torrent(&file).unwrap();
        assert_eq!(meta.name, "file.bin");
        assert_eq!(meta.piece_length, 16384);
        assert_eq!(meta.piece_count, 1);
        assert_eq!(meta.total_bytes, 1024);
        assert_eq!(meta.files.len(), 1);
        assert_eq!(meta.announce.as_deref(), Some("http://tracker/ann"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bencode_rejects_nonsense() {
        let dir = scratch("bencode-bad");
        let file = dir.join("bad.torrent");
        fs::write(&file, b"not a torrent").unwrap();
        assert!(parse_torrent(&file).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn probe_reports_a_refusal_rather_than_failing() {
        // Port 1 on localhost is almost certainly closed; the point is that a
        // refused connection is a result, not an error.
        let result = probe_tcp("127.0.0.1", 1, 300).unwrap();
        assert!(!result.connected);
        assert!(!result.detail.is_empty());
    }

    #[test]
    fn upnp_payloads_are_well_formed() {
        let payloads = upnp_payloads(8080, 80, "192.168.1.10", "tcp");
        assert!(payloads.ssdp_discovery.starts_with("M-SEARCH * HTTP/1.1"));
        assert!(payloads.soap_add_port_mapping.contains("<NewExternalPort>8080</NewExternalPort>"));
        assert!(payloads.soap_add_port_mapping.contains("<NewProtocol>TCP</NewProtocol>"));
        assert!(payloads.soap_add_port_mapping.contains("192.168.1.10"));
    }

    #[test]
    fn split_rejects_a_zero_chunk_size() {
        let dir = scratch("split-zero");
        let source = dir.join("x.bin");
        fs::write(&source, b"data").unwrap();
        assert!(split_file(&source, 0, &dir.join("parts")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
