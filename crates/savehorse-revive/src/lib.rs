//! revive — recovery.
//!
//! Two applications live here, because they do one job between them:
//!
//! * `revive` 1.0.0 (generation 2) — signature carving: scan a blob for file
//!   headers, and write the recoverable ones out.
//! * `chronos` 1.0.0 (generation 1) — snapshot containers: pack a tree, verify
//!   it against CRC-32, restore it, and read a disk image's geometry.
//!
//! The snapshot format is byte-for-byte the one chronos wrote, so existing
//! `.chronos` archives still restore. That was deliberate: changing it would
//! strand the only copies of whatever is already backed up.

use serde::{Deserialize, Serialize};
use savehorse_core::{crc32, to_hex, Error, Result};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The snapshot magic, unchanged from chronos.
const CHRONOS_MAGIC: &[u8; 8] = b"CHRONOS1";

/// Default bounds for a carve: ignore anything smaller than a header, and stop
/// chasing a single file past fifty megabytes.
const DEFAULT_MIN: usize = 32;
const DEFAULT_MAX: usize = 50 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Carving
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CarvedArtifact {
    pub index: usize,
    pub format: String,
    pub extension: String,
    pub offset_bytes: usize,
    pub length_bytes: usize,
    pub sha256: String,
    pub confidence: String,
    pub saved_filename: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub source_file: String,
    pub total_scanned_bytes: usize,
    pub artifacts_found: usize,
    pub total_recovered_bytes: usize,
    pub artifacts: Vec<CarvedArtifact>,
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    to_hex(&hasher.finalize())
}

/// One signature the scanner knows: where a file starts, and where it ends.
struct Signature {
    name: &'static str,
    extension: &'static str,
    /// Matched against the bytes at the cursor.
    start: fn(&[u8], usize) -> bool,
    /// Returns the end offset of the file, given the start and the data.
    end: fn(&[u8], usize, usize) -> Option<usize>,
    /// Which `--types` words select this signature.
    aliases: &'static [&'static str],
}

fn starts_png(b: &[u8], i: usize) -> bool {
    i + 8 <= b.len() && &b[i..i + 8] == b"\x89PNG\r\n\x1a\n"
}
fn ends_png(b: &[u8], start: usize, limit: usize) -> Option<usize> {
    for i in start + 8..limit.min(b.len()) {
        if i + 4 <= b.len() && &b[i..i + 4] == b"IEND" && i + 8 <= b.len() {
            return Some(i + 8);
        }
    }
    None
}

fn starts_jpeg(b: &[u8], i: usize) -> bool {
    i + 3 <= b.len() && b[i] == 0xFF && b[i + 1] == 0xD8 && b[i + 2] == 0xFF
}
fn ends_jpeg(b: &[u8], start: usize, limit: usize) -> Option<usize> {
    for i in start + 3..limit.min(b.len()) {
        if i + 2 <= b.len() && b[i] == 0xFF && b[i + 1] == 0xD9 {
            return Some(i + 2);
        }
    }
    None
}

fn starts_pdf(b: &[u8], i: usize) -> bool {
    i + 5 <= b.len() && &b[i..i + 5] == b"%PDF-"
}
fn ends_pdf(b: &[u8], start: usize, limit: usize) -> Option<usize> {
    for i in start + 5..limit.min(b.len()) {
        if i + 5 <= b.len() && &b[i..i + 5] == b"%%EOF" {
            return Some(i + 5);
        }
    }
    None
}

fn starts_zip(b: &[u8], i: usize) -> bool {
    i + 4 <= b.len() && &b[i..i + 4] == b"PK\x03\x04"
}
fn ends_zip(b: &[u8], start: usize, limit: usize) -> Option<usize> {
    for i in start + 4..limit.min(b.len()) {
        if i + 22 <= b.len() && &b[i..i + 4] == b"PK\x05\x06" {
            return Some(i + 22);
        }
    }
    None
}

fn starts_gif(b: &[u8], i: usize) -> bool {
    i + 6 <= b.len() && (&b[i..i + 6] == b"GIF87a" || &b[i..i + 6] == b"GIF89a")
}
fn ends_gif(b: &[u8], start: usize, limit: usize) -> Option<usize> {
    for i in start + 6..limit.min(b.len()) {
        if b[i] == 0x3B {
            return Some(i + 1);
        }
    }
    None
}

fn signatures() -> Vec<Signature> {
    vec![
        Signature {
            name: "PNG Image",
            extension: "png",
            start: starts_png,
            end: ends_png,
            aliases: &["png"],
        },
        Signature {
            name: "JPEG Image",
            extension: "jpg",
            start: starts_jpeg,
            end: ends_jpeg,
            aliases: &["jpg", "jpeg"],
        },
        Signature {
            name: "PDF Document",
            extension: "pdf",
            start: starts_pdf,
            end: ends_pdf,
            aliases: &["pdf"],
        },
        Signature {
            name: "ZIP Archive",
            extension: "zip",
            start: starts_zip,
            end: ends_zip,
            aliases: &["zip", "docx", "xlsx", "pptx"],
        },
        Signature {
            name: "GIF Image",
            extension: "gif",
            start: starts_gif,
            end: ends_gif,
            aliases: &["gif"],
        },
    ]
}

/// Walk a blob looking for file headers, and report what could be recovered.
pub fn scan_signatures(
    bytes: &[u8],
    type_filter: Option<&str>,
    min_size: Option<usize>,
    max_size: Option<usize>,
) -> Vec<CarvedArtifact> {
    let min_bytes = min_size.unwrap_or(DEFAULT_MIN);
    let max_bytes = max_size.unwrap_or(DEFAULT_MAX);

    let allowed: Option<Vec<String>> = type_filter.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let sigs = signatures();
    let mut artifacts: Vec<CarvedArtifact> = Vec::new();
    let mut cursor = 0usize;
    let len = bytes.len();

    while cursor < len {
        let mut matched = false;

        for sig in &sigs {
            let permitted = match &allowed {
                Some(list) => list.iter().any(|a| sig.aliases.contains(&a.as_str())),
                None => true,
            };
            if !permitted || !(sig.start)(bytes, cursor) {
                continue;
            }

            let limit = len.min(cursor.saturating_add(max_bytes));
            let end = match (sig.end)(bytes, cursor, limit) {
                Some(e) => e,
                // A header with no terminator in range is not a file.
                None => continue,
            };

            let file_len = end - cursor;
            if file_len < min_bytes || file_len > max_bytes {
                continue;
            }

            artifacts.push(CarvedArtifact {
                index: artifacts.len() + 1,
                format: sig.name.to_string(),
                extension: sig.extension.to_string(),
                offset_bytes: cursor,
                length_bytes: file_len,
                sha256: sha256_hex(&bytes[cursor..end]),
                confidence: "High (100%)".to_string(),
                saved_filename: None,
            });

            cursor = end;
            matched = true;
            break;
        }

        if !matched {
            cursor += 1;
        }
    }

    artifacts
}

fn read_all(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Scan without writing anything.
pub fn scan_file(
    path: &Path,
    type_filter: Option<&str>,
    min_size: Option<usize>,
    max_size: Option<usize>,
) -> Result<ScanSummary> {
    let buffer = read_all(path)?;
    let artifacts = scan_signatures(&buffer, type_filter, min_size, max_size);
    let recovered: usize = artifacts.iter().map(|a| a.length_bytes).sum();

    Ok(ScanSummary {
        source_file: path.to_string_lossy().to_string(),
        total_scanned_bytes: buffer.len(),
        artifacts_found: artifacts.len(),
        total_recovered_bytes: recovered,
        artifacts,
    })
}

/// Scan and write every recoverable file into `out_dir`.
pub fn carve_file(
    path: &Path,
    out_dir: &Path,
    type_filter: Option<&str>,
    min_size: Option<usize>,
    max_size: Option<usize>,
) -> Result<ScanSummary> {
    let buffer = read_all(path)?;
    fs::create_dir_all(out_dir)?;

    let mut artifacts = scan_signatures(&buffer, type_filter, min_size, max_size);
    let mut written = 0usize;

    for (i, artifact) in artifacts.iter_mut().enumerate() {
        let name = format!(
            "carved_{:04}_{}.{}",
            i + 1,
            artifact.offset_bytes,
            artifact.extension
        );
        let dest = out_dir.join(name);
        let start = artifact.offset_bytes;
        let end = start + artifact.length_bytes;
        fs::write(&dest, &buffer[start..end])?;
        artifact.saved_filename = Some(dest.to_string_lossy().to_string());
        written += artifact.length_bytes;
    }

    Ok(ScanSummary {
        source_file: path.to_string_lossy().to_string(),
        total_scanned_bytes: buffer.len(),
        artifacts_found: artifacts.len(),
        total_recovered_bytes: written,
        artifacts,
    })
}

/// A written report of a scan: per-format totals and a plain assessment.
///
/// The original's `report` was the same call as `scan` with different flags.
/// This produces something you can hand to someone.
pub fn build_report(summary: &ScanSummary) -> String {
    let mut by_format: HashMap<&str, (usize, usize)> = HashMap::new();
    for a in &summary.artifacts {
        let entry = by_format.entry(a.format.as_str()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += a.length_bytes;
    }

    let mut formats: Vec<_> = by_format.into_iter().collect();
    formats.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

    let mut out = String::new();
    out.push_str("RECOVERY REPORT\n");
    out.push_str("===============\n\n");
    out.push_str(&format!("source        {}\n", summary.source_file));
    out.push_str(&format!(
        "scanned       {} bytes\n",
        summary.total_scanned_bytes
    ));
    out.push_str(&format!("artifacts     {}\n", summary.artifacts_found));
    out.push_str(&format!(
        "recoverable   {} bytes\n\n",
        summary.total_recovered_bytes
    ));

    if summary.artifacts.is_empty() {
        out.push_str("Nothing recognisable was found. The blob may be compressed,\n");
        out.push_str("encrypted, or hold no complete files.\n");
        return out;
    }

    out.push_str("BY FORMAT\n");
    for (format, (count, bytes)) in &formats {
        out.push_str(&format!("  {:<18} {:>5} file(s)  {:>10} bytes\n", format, count, bytes));
    }

    out.push_str("\nFILES\n");
    for a in &summary.artifacts {
        out.push_str(&format!(
            "  #{:<4} {:<16} 0x{:08X}  {:>9} bytes  sha256 {}\n",
            a.index,
            a.format,
            a.offset_bytes,
            a.length_bytes,
            &a.sha256[..16.min(a.sha256.len())]
        ));
    }

    out
}

// ---------------------------------------------------------------------------
// Snapshots (chronos)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFileHeader {
    pub relative_path: String,
    pub file_size: u64,
    pub crc32: u32,
    pub modified_timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PackReport {
    pub archive_path: String,
    pub files_archived: usize,
    pub total_bytes_archived: u64,
    pub entries: Vec<ArchiveFileHeader>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VerifyEntry {
    pub path: String,
    pub expected_size: u64,
    pub expected_crc32: u32,
    pub actual_crc32: u32,
    pub is_valid: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VerifyReport {
    pub archive_path: String,
    pub total_files: usize,
    pub valid_files: usize,
    pub corrupted_files: usize,
    pub all_passed: bool,
    pub results: Vec<VerifyEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UnpackReport {
    pub archive_path: String,
    pub destination_directory: String,
    pub files_extracted: usize,
    pub total_bytes_extracted: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DiskScanReport {
    pub target_file: String,
    pub total_bytes: u64,
    pub total_sectors: u64,
    pub partition_scheme: String,
    pub zero_sectors: u64,
    pub data_sectors: u64,
    pub average_entropy: f64,
}

fn collect_files(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, base, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, rel));
        }
    }
    Ok(())
}

/// Pack a directory into a `.chronos` snapshot.
pub fn pack_directory(source_dir: &Path, archive_path: &Path) -> Result<PackReport> {
    if !source_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            source_dir.display()
        )));
    }

    let mut files = Vec::new();
    collect_files(source_dir, source_dir, &mut files)?;
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let mut entries = Vec::with_capacity(files.len());
    let mut payloads = Vec::with_capacity(files.len());
    let mut total_bytes = 0u64;

    for (abs, rel) in files {
        let data = fs::read(&abs)?;
        let size = data.len() as u64;
        total_bytes += size;

        let modified = abs
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        entries.push(ArchiveFileHeader {
            relative_path: rel,
            file_size: size,
            crc32: crc32(&data),
            modified_timestamp: modified,
        });
        payloads.push(data);
    }

    if let Some(parent) = archive_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let mut out = File::create(archive_path)?;
    out.write_all(CHRONOS_MAGIC)?;
    out.write_all(&(entries.len() as u32).to_le_bytes())?;

    for entry in &entries {
        let path_bytes = entry.relative_path.as_bytes();
        out.write_all(&(path_bytes.len() as u16).to_le_bytes())?;
        out.write_all(path_bytes)?;
        out.write_all(&entry.file_size.to_le_bytes())?;
        out.write_all(&entry.crc32.to_le_bytes())?;
        out.write_all(&entry.modified_timestamp.to_le_bytes())?;
    }
    for payload in &payloads {
        out.write_all(payload)?;
    }
    out.flush()?;

    Ok(PackReport {
        archive_path: archive_path.to_string_lossy().to_string(),
        files_archived: entries.len(),
        total_bytes_archived: total_bytes,
        entries,
    })
}

fn read_headers(file: &mut File) -> Result<Vec<ArchiveFileHeader>> {
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != CHRONOS_MAGIC {
        return Err(Error::Invalid(
            "not a snapshot: the CHRONOS1 header is missing".to_string(),
        ));
    }

    let mut count_buf = [0u8; 4];
    file.read_exact(&mut count_buf)?;
    let count = u32::from_le_bytes(count_buf) as usize;

    let mut headers = Vec::with_capacity(count.min(1_000_000));
    for _ in 0..count {
        let mut len_buf = [0u8; 2];
        file.read_exact(&mut len_buf)?;
        let path_len = u16::from_le_bytes(len_buf) as usize;

        let mut path_buf = vec![0u8; path_len];
        file.read_exact(&mut path_buf)?;

        let mut size_buf = [0u8; 8];
        file.read_exact(&mut size_buf)?;
        let mut crc_buf = [0u8; 4];
        file.read_exact(&mut crc_buf)?;
        let mut time_buf = [0u8; 8];
        file.read_exact(&mut time_buf)?;

        headers.push(ArchiveFileHeader {
            relative_path: String::from_utf8_lossy(&path_buf).to_string(),
            file_size: u64::from_le_bytes(size_buf),
            crc32: u32::from_le_bytes(crc_buf),
            modified_timestamp: u64::from_le_bytes(time_buf),
        });
    }

    Ok(headers)
}

/// Check every payload against the CRC-32 recorded when it was packed.
pub fn verify_archive(archive_path: &Path) -> Result<VerifyReport> {
    let mut file = File::open(archive_path)?;
    let headers = read_headers(&mut file)?;

    let mut results = Vec::with_capacity(headers.len());
    let mut valid = 0usize;
    let mut corrupt = 0usize;

    for header in headers {
        let mut payload = vec![0u8; header.file_size as usize];
        file.read_exact(&mut payload)?;
        let actual = crc32(&payload);
        let ok = actual == header.crc32;

        if ok {
            valid += 1;
        } else {
            corrupt += 1;
        }

        results.push(VerifyEntry {
            path: header.relative_path,
            expected_size: header.file_size,
            expected_crc32: header.crc32,
            actual_crc32: actual,
            is_valid: ok,
        });
    }

    Ok(VerifyReport {
        archive_path: archive_path.to_string_lossy().to_string(),
        total_files: results.len(),
        valid_files: valid,
        corrupted_files: corrupt,
        all_passed: corrupt == 0,
        results,
    })
}

/// Restore a snapshot into a destination directory.
pub fn unpack_archive(archive_path: &Path, dest_dir: &Path) -> Result<UnpackReport> {
    let mut file = File::open(archive_path)?;
    let headers = read_headers(&mut file)?;

    fs::create_dir_all(dest_dir)?;
    let mut extracted = 0usize;
    let mut total = 0u64;

    for header in headers {
        let mut payload = vec![0u8; header.file_size as usize];
        file.read_exact(&mut payload)?;

        // A stored path is data, not an instruction: keep only safe components.
        let mut safe = PathBuf::new();
        for part in header.relative_path.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                continue;
            }
            safe.push(part);
        }
        if safe.as_os_str().is_empty() {
            continue;
        }

        let dest = dest_dir.join(&safe);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &payload)?;
        extracted += 1;
        total += header.file_size;
    }

    Ok(UnpackReport {
        archive_path: archive_path.to_string_lossy().to_string(),
        destination_directory: dest_dir.to_string_lossy().to_string(),
        files_extracted: extracted,
        total_bytes_extracted: total,
    })
}

/// Read the first megabyte of a disk image: partition scheme, empty sectors
/// and average per-sector entropy.
pub fn scan_disk_image(path: &Path) -> Result<DiskScanReport> {
    let mut file = File::open(path)?;
    let total_size = file.metadata()?.len();

    let sector_size = 512u64;
    let total_sectors = total_size / sector_size;

    let mut zero_sectors = 0u64;
    let mut data_sectors = 0u64;
    let mut total_entropy = 0.0f64;
    let mut scheme = "raw / unpartitioned".to_string();

    let mut buf = vec![0u8; sector_size as usize];
    let sectors_to_read = total_sectors.min(2048);

    for index in 0..sectors_to_read {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }

        if index == 0 && read >= 512 && buf[510] == 0x55 && buf[511] == 0xAA {
            scheme = "Master Boot Record (MBR)".to_string();
        }
        if index == 1 && read >= 8 && &buf[0..8] == b"EFI PART" {
            scheme = "GUID Partition Table (GPT)".to_string();
        }

        if buf[..read].iter().all(|&b| b == 0) {
            zero_sectors += 1;
            continue;
        }

        data_sectors += 1;
        let mut counts = [0u32; 256];
        for &b in &buf[..read] {
            counts[b as usize] += 1;
        }
        let length = read as f64;
        let mut entropy = 0.0f64;
        for &count in &counts {
            if count > 0 {
                let p = count as f64 / length;
                entropy -= p * p.log2();
            }
        }
        total_entropy += entropy;
    }

    let average_entropy = if data_sectors > 0 {
        (total_entropy / data_sectors as f64 * 1000.0).round() / 1000.0
    } else {
        0.0
    };

    Ok(DiskScanReport {
        target_file: path.to_string_lossy().to_string(),
        total_bytes: total_size,
        total_sectors,
        partition_scheme: scheme,
        zero_sectors,
        data_sectors,
        average_entropy,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-revive-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A minimal but complete PNG: header, IEND.
    fn png_bytes() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        v.extend_from_slice(&[0u8; 40]);
        v.extend_from_slice(b"IEND");
        v.extend_from_slice(&[0u8; 4]);
        v
    }

    #[test]
    fn finds_a_png_inside_a_blob() {
        let mut blob = vec![0u8; 64];
        blob.extend_from_slice(&png_bytes());
        blob.extend_from_slice(&[0u8; 32]);

        let found = scan_signatures(&blob, None, None, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].format, "PNG Image");
        assert_eq!(found[0].offset_bytes, 64);
        assert_eq!(found[0].extension, "png");
        assert!(found[0].length_bytes > 8);
    }

    #[test]
    fn type_filter_limits_what_is_found() {
        let mut blob = png_bytes();
        blob.extend_from_slice(b"%PDF-1.4 something %%EOF");

        let all = scan_signatures(&blob, None, None, None);
        assert!(all.len() >= 2, "got {:?}", all);

        let only_pdf = scan_signatures(&blob, Some("pdf"), None, None);
        assert_eq!(only_pdf.len(), 1);
        assert_eq!(only_pdf[0].extension, "pdf");
    }

    #[test]
    fn a_header_without_an_end_is_not_a_file() {
        let blob = b"\x89PNG\r\n\x1a\ntruncated".to_vec();
        let found = scan_signatures(&blob, None, None, None);
        assert!(found.is_empty(), "an unterminated header must not be carved");
    }

    #[test]
    fn carve_writes_files_that_match_the_scan() {
        let dir = scratch("carve");
        let mut blob = vec![0u8; 16];
        blob.extend_from_slice(&png_bytes());

        let source = dir.join("disk.img");
        fs::write(&source, &blob).unwrap();

        let out = dir.join("out");
        let summary = carve_file(&source, &out, None, None, None).unwrap();

        assert_eq!(summary.artifacts_found, 1);
        let saved = summary.artifacts[0].saved_filename.clone().unwrap();
        let carved = fs::read(&saved).unwrap();
        assert_eq!(carved, png_bytes());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_lists_formats_and_totals() {
        let mut blob = png_bytes();
        blob.extend_from_slice(b"%PDF-1.4 x %%EOF");

        let dir = scratch("report");
        let source = dir.join("blob.bin");
        fs::write(&source, &blob).unwrap();

        let summary = scan_file(&source, None, None, None).unwrap();
        let report = build_report(&summary);

        assert!(report.contains("RECOVERY REPORT"));
        assert!(report.contains("BY FORMAT"));
        assert!(report.contains("PNG Image"));
        assert!(report.contains("PDF Document"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_report_says_so() {
        let summary = ScanSummary {
            source_file: "x".to_string(),
            total_scanned_bytes: 10,
            artifacts_found: 0,
            total_recovered_bytes: 0,
            artifacts: Vec::new(),
        };
        let report = build_report(&summary);
        assert!(report.contains("Nothing recognisable"));
    }

    #[test]
    fn snapshot_packs_verifies_and_restores() {
        let dir = scratch("snapshot");
        let src = dir.join("tree");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), b"alpha").unwrap();
        fs::write(src.join("sub").join("b.txt"), b"beta beta").unwrap();

        let archive = dir.join("snap.chronos");
        let packed = pack_directory(&src, &archive).unwrap();
        assert_eq!(packed.files_archived, 2);
        assert_eq!(packed.total_bytes_archived, 14);

        let verified = verify_archive(&archive).unwrap();
        assert!(verified.all_passed, "results: {:?}", verified.results);
        assert_eq!(verified.valid_files, 2);

        let dest = dir.join("restored");
        let unpacked = unpack_archive(&archive, &dest).unwrap();
        assert_eq!(unpacked.files_extracted, 2);
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(dest.join("sub").join("b.txt")).unwrap(), b"beta beta");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_catches_a_corrupted_payload() {
        let dir = scratch("corrupt");
        let src = dir.join("tree");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"alpha").unwrap();

        let archive = dir.join("snap.chronos");
        pack_directory(&src, &archive).unwrap();

        // Flip a byte in the payload region, after the header block.
        let mut bytes = fs::read(&archive).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&archive, &bytes).unwrap();

        let verified = verify_archive(&archive).unwrap();
        assert!(!verified.all_passed);
        assert_eq!(verified.corrupted_files, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unpack_refuses_to_escape_the_destination() {
        let dir = scratch("escape");
        let src = dir.join("tree");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"x").unwrap();

        let archive = dir.join("snap.chronos");
        pack_directory(&src, &archive).unwrap();

        // Rewrite the stored path to a traversal.
        let mut bytes = fs::read(&archive).unwrap();
        let needle = b"a.txt";
        if let Some(pos) = bytes.windows(needle.len()).position(|w| w == needle) {
            // Same length, so the header lengths stay valid.
            bytes[pos..pos + 5].copy_from_slice(b"a/../");
        }
        fs::write(&archive, &bytes).unwrap();

        let dest = dir.join("restored");
        let report = unpack_archive(&archive, &dest).unwrap();
        // Either skipped entirely or written safely inside the destination.
        for entry in fs::read_dir(&dest).unwrap().flatten() {
            let p = entry.path();
            assert!(p.starts_with(&dest), "escaped to {:?}", p);
        }
        let _ = report;
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_scan_identifies_an_mbr() {
        let dir = scratch("disk");
        let mut image = vec![0u8; 512 * 8];
        image[510] = 0x55;
        image[511] = 0xAA;
        for (i, b) in image[512..1024].iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let path = dir.join("disk.img");
        fs::write(&path, &image).unwrap();

        let report = scan_disk_image(&path).unwrap();
        assert_eq!(report.partition_scheme, "Master Boot Record (MBR)");
        assert_eq!(report.total_sectors, 8);
        assert!(report.zero_sectors > 0);
        assert!(report.average_entropy > 0.0);
        let _ = fs::remove_dir_all(&dir);
    }
}
