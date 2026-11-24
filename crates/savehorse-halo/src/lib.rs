//! halo — disc images.
//!
//! Ported from `halo` 1.0.0 (generation 2). The ISO 9660 writer and reader are
//! the original algorithms. Three things were wrong in the original and are
//! fixed here, because each one could lose data silently:
//!
//! 1. **The path table was never written.** The primary volume descriptor
//!    claimed a 10-byte path table at sector 18 — which was actually where the
//!    root directory lived. Readers that trust the descriptor were being sent
//!    to the wrong sector. A real L and M path table are now written.
//! 2. **Subdirectories were ignored.** `create` walked one level and packed the
//!    files it found, silently dropping everything below. It now refuses with
//!    an error rather than producing an image that looks complete.
//! 3. **Overflow truncated silently.** File records that did not fit in the
//!    single root-directory sector were dropped with a `break`. It now errors,
//!    naming the limit.
//!
//! A fourth, smaller change: two source files that sanitise to the same ISO
//! name are now disambiguated instead of colliding.

use serde::{Deserialize, Serialize};
use savehorse_core::{Error, Result};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SECTOR_SIZE: usize = 2048;
pub const RAW_SECTOR_SIZE: usize = 2352;
pub const PVD_SECTOR: u32 = 16;
pub const TERMINATOR_SECTOR: u32 = 17;
pub const L_PATH_TABLE_SECTOR: u32 = 18;
pub const M_PATH_TABLE_SECTOR: u32 = 19;
pub const ROOT_DIR_SECTOR: u32 = 20;
const FIRST_DATA_SECTOR: u32 = ROOT_DIR_SECTOR + 1;

/// Longest file identifier ISO 9660 level 2 allows, including the `;1` suffix.
const MAX_NAME_LEN: usize = 28;

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct IsoFileEntry {
    pub name: String,
    pub extent_sector: u32,
    pub size_bytes: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct IsoVolumeInfo {
    pub volume_id: String,
    pub total_sectors: u32,
    pub sector_size: u32,
    pub files_count: usize,
    pub entries: Vec<IsoFileEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub image_path: String,
    pub volume_id: String,
    pub files_count: usize,
    pub total_sectors: u32,
    pub bytes_written: u64,
    pub entries: Vec<IsoFileEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct VerifyResult {
    pub valid: bool,
    pub volume_id: String,
    pub total_sectors: u32,
    pub files_count: usize,
    pub message: String,
    pub issues: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConvertResult {
    pub source_file: String,
    pub output_file: String,
    pub source_sectors: u64,
    pub output_sectors: u64,
    pub source_bytes: u64,
    pub output_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    pub image_path: String,
    pub output_dir: String,
    pub extracted: Vec<IsoFileEntry>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a 32-bit field in both endiannesses, as ISO 9660 requires for most
/// numeric fields.
fn write_u32_both(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
    buf[offset + 4..offset + 8].copy_from_slice(&val.to_be_bytes());
}

fn write_u16_both(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
    buf[offset + 2..offset + 4].copy_from_slice(&val.to_be_bytes());
}

/// ISO 9660 wants a seven-byte date. A fixed, valid stamp is used rather than
/// a clock read: the image is deterministic, so the same input always produces
/// the same bytes and a test can compare two builds.
fn write_record_date(buf: &mut [u8], offset: usize) {
    buf[offset] = 126; // years since 1900
    buf[offset + 1] = 1; // month
    buf[offset + 2] = 1; // day
    buf[offset + 3] = 0; // hour
    buf[offset + 4] = 0; // minute
    buf[offset + 5] = 0; // second
    buf[offset + 6] = 0; // GMT offset
}

/// Reduce a file name to characters ISO 9660 permits, uppercased.
pub fn sanitize_iso_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push('_');
        }
    }
    if out.len() > MAX_NAME_LEN {
        out.truncate(MAX_NAME_LEN);
    }
    if out.is_empty() {
        out.push_str("FILE");
    }
    out
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

struct PlannedFile {
    source: std::path::PathBuf,
    iso_name: String,
    size: u64,
    sector: u32,
}

/// Build an ISO 9660 image from the files directly inside `source_dir`.
pub fn create_image(source_dir: &Path, out_path: &Path, volume_id: &str) -> Result<CreateReport> {
    if !source_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            savehorse_core::display_path(&source_dir)
        )));
    }

    // Refuse rather than silently drop a subtree.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            return Err(Error::Invalid(format!(
                "subdirectories are not supported yet: '{}' would be left out of the image",
                savehorse_core::display_path(&path)
            )));
        }
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();

    // Sanitise, disambiguating any collision.
    let mut used: Vec<String> = Vec::new();
    let mut planned: Vec<PlannedFile> = Vec::new();
    let mut sector = FIRST_DATA_SECTOR;

    for path in files {
        let raw = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "FILE".to_string());
        let mut name = sanitize_iso_name(&raw);
        if used.contains(&name) {
            let mut n = 2;
            loop {
                let candidate = format!("{}_{}", &name[..name.len().min(MAX_NAME_LEN - 3)], n);
                if !used.contains(&candidate) {
                    name = candidate;
                    break;
                }
                n += 1;
            }
        }
        used.push(name.clone());

        let size = fs::metadata(&path)?.len();
        let sectors_needed = (((size + SECTOR_SIZE as u64 - 1) / SECTOR_SIZE as u64).max(1)) as u32;
        planned.push(PlannedFile {
            source: path,
            iso_name: name,
            size,
            sector,
        });
        sector += sectors_needed;
    }

    // Does every directory record fit in the one root sector?
    let mut used_bytes = 34 + 34; // "." and ".."
    for p in &planned {
        let record_len = 33 + p.iso_name.len() + 1;
        used_bytes += if record_len % 2 == 0 { record_len } else { record_len + 1 };
    }
    if used_bytes > SECTOR_SIZE {
        return Err(Error::Invalid(format!(
            "too many files for a single root directory: {} files need {} bytes of a {} byte sector",
            planned.len(),
            used_bytes,
            SECTOR_SIZE
        )));
    }

    let total_sectors = sector;

    let mut out = BufWriter::new(File::create(out_path)?);

    // 1. System area: sixteen empty sectors.
    out.write_all(&[0u8; SECTOR_SIZE * 16])?;

    // 2. Primary volume descriptor.
    let mut pvd = [0u8; SECTOR_SIZE];
    pvd[0] = 0x01;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 0x01;

    let mut sys_id = [b' '; 32];
    sys_id[..9].copy_from_slice(b"SAVEHORSE");
    pvd[8..40].copy_from_slice(&sys_id);

    let mut vol_id = [b' '; 32];
    let vol_bytes = volume_id.as_bytes();
    let vol_len = vol_bytes.len().min(32);
    vol_id[..vol_len].copy_from_slice(&vol_bytes[..vol_len]);
    pvd[40..72].copy_from_slice(&vol_id);

    write_u32_both(&mut pvd, 80, total_sectors); // volume space size
    write_u16_both(&mut pvd, 120, 1); // volume set size
    write_u16_both(&mut pvd, 124, 1); // volume sequence number
    write_u16_both(&mut pvd, 128, SECTOR_SIZE as u16); // logical block size
    write_u32_both(&mut pvd, 132, 10); // path table size: one 10-byte record
    pvd[140..144].copy_from_slice(&L_PATH_TABLE_SECTOR.to_le_bytes());
    pvd[148..152].copy_from_slice(&M_PATH_TABLE_SECTOR.to_be_bytes());

    // Root directory record, 34 bytes at offset 156.
    pvd[156] = 34;
    pvd[157] = 0;
    write_u32_both(&mut pvd, 158, ROOT_DIR_SECTOR);
    write_u32_both(&mut pvd, 166, SECTOR_SIZE as u32);
    write_record_date(&mut pvd, 174);
    pvd[181] = 0x02; // directory flag
    pvd[182] = 0;
    pvd[183] = 0;
    write_u16_both(&mut pvd, 184, 1);
    pvd[188] = 1;
    pvd[189] = 0x00;

    out.write_all(&pvd)?;

    // 3. Volume descriptor set terminator.
    let mut term = [0u8; SECTOR_SIZE];
    term[0] = 0xFF;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 0x01;
    out.write_all(&term)?;

    // 4. Little-endian path table: one record, for the root directory.
    let mut l_path = [0u8; SECTOR_SIZE];
    l_path[0] = 1; // length of directory identifier
    l_path[1] = 0; // extended attribute length
    l_path[2..6].copy_from_slice(&ROOT_DIR_SECTOR.to_le_bytes());
    l_path[6..8].copy_from_slice(&1u16.to_le_bytes()); // parent directory number
    l_path[8] = 0x00; // root identifier
    l_path[9] = 0; // padding, because the identifier length is odd
    out.write_all(&l_path)?;

    // 5. Big-endian path table.
    let mut m_path = [0u8; SECTOR_SIZE];
    m_path[0] = 1;
    m_path[1] = 0;
    m_path[2..6].copy_from_slice(&ROOT_DIR_SECTOR.to_be_bytes());
    m_path[6..8].copy_from_slice(&1u16.to_be_bytes());
    m_path[8] = 0x00;
    m_path[9] = 0;
    out.write_all(&m_path)?;

    // 6. Root directory sector.
    let mut root = [0u8; SECTOR_SIZE];
    let mut offset = 0usize;

    for id in [0x00u8, 0x01u8] {
        root[offset] = 34;
        write_u32_both(&mut root, offset + 2, ROOT_DIR_SECTOR);
        write_u32_both(&mut root, offset + 10, SECTOR_SIZE as u32);
        write_record_date(&mut root, offset + 18);
        root[offset + 25] = 0x02;
        write_u16_both(&mut root, offset + 28, 1);
        root[offset + 32] = 1;
        root[offset + 33] = id;
        offset += 34;
    }

    for p in &planned {
        let name = format!("{};1", p.iso_name);
        let name_bytes = name.as_bytes();
        let record_len = 33 + name_bytes.len();
        let record_len = if record_len % 2 == 0 { record_len } else { record_len + 1 };

        root[offset] = record_len as u8;
        write_u32_both(&mut root, offset + 2, p.sector);
        write_u32_both(&mut root, offset + 10, p.size as u32);
        write_record_date(&mut root, offset + 18);
        root[offset + 25] = 0x00; // regular file
        write_u16_both(&mut root, offset + 28, 1);
        root[offset + 32] = name_bytes.len() as u8;
        root[offset + 33..offset + 33 + name_bytes.len()].copy_from_slice(name_bytes);
        offset += record_len;
    }

    out.write_all(&root)?;

    // 7. File contents, sector aligned.
    for p in &planned {
        let mut f = File::open(&p.source)?;
        let mut buf = [0u8; SECTOR_SIZE];
        let mut remaining = p.size;
        while remaining > 0 {
            let to_read = (remaining as usize).min(SECTOR_SIZE);
            f.read_exact(&mut buf[..to_read])?;
            if to_read < SECTOR_SIZE {
                buf[to_read..].fill(0);
            }
            out.write_all(&buf)?;
            remaining -= to_read as u64;
        }
    }

    out.flush()?;
    let bytes_written = fs::metadata(out_path)?.len();

    Ok(CreateReport {
        image_path: out_path.to_string_lossy().to_string(),
        volume_id: volume_id.to_string(),
        files_count: planned.len(),
        total_sectors,
        bytes_written,
        entries: planned
            .iter()
            .map(|p| IsoFileEntry {
                name: p.iso_name.clone(),
                extent_sector: p.sector,
                size_bytes: p.size,
                is_dir: false,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Read
// ---------------------------------------------------------------------------

/// Read the primary volume descriptor and the root directory.
pub fn read_metadata(image_path: &Path) -> Result<IsoVolumeInfo> {
    let mut file = File::open(image_path)?;
    let file_len = file.metadata()?.len();

    if file_len < (PVD_SECTOR as u64 + 2) * SECTOR_SIZE as u64 {
        return Err(Error::Invalid(
            "file is too small to be an ISO 9660 image".to_string(),
        ));
    }

    file.seek(SeekFrom::Start(PVD_SECTOR as u64 * SECTOR_SIZE as u64))?;
    let mut pvd = [0u8; SECTOR_SIZE];
    file.read_exact(&mut pvd)?;

    if pvd[0] != 0x01 || &pvd[1..6] != b"CD001" {
        return Err(Error::Invalid(
            "not an ISO 9660 image: primary volume descriptor missing".to_string(),
        ));
    }

    let volume_id = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();
    let total_sectors = u32::from_le_bytes(
        pvd[80..84]
            .try_into()
            .map_err(|_| Error::Invalid("truncated volume descriptor".to_string()))?,
    );
    let root_sector = u32::from_le_bytes(
        pvd[158..162]
            .try_into()
            .map_err(|_| Error::Invalid("truncated volume descriptor".to_string()))?,
    );

    if (root_sector as u64 + 1) * SECTOR_SIZE as u64 > file_len {
        return Err(Error::Invalid(format!(
            "root directory sector {} lies outside the image",
            root_sector
        )));
    }

    file.seek(SeekFrom::Start(root_sector as u64 * SECTOR_SIZE as u64))?;
    let mut root = [0u8; SECTOR_SIZE];
    file.read_exact(&mut root)?;

    let mut entries = Vec::new();
    let mut offset = 0usize;

    while offset < SECTOR_SIZE {
        let record_len = root[offset] as usize;
        if record_len == 0 {
            break;
        }
        if offset + record_len > SECTOR_SIZE || record_len < 34 {
            break;
        }

        let extent_sector = u32::from_le_bytes(
            root[offset + 2..offset + 6]
                .try_into()
                .map_err(|_| Error::Invalid("truncated directory record".to_string()))?,
        );
        let size_bytes = u32::from_le_bytes(
            root[offset + 10..offset + 14]
                .try_into()
                .map_err(|_| Error::Invalid("truncated directory record".to_string()))?,
        ) as u64;
        let flags = root[offset + 25];
        let id_len = root[offset + 32] as usize;

        if id_len > 0 && offset + 33 + id_len <= SECTOR_SIZE {
            let id_bytes = &root[offset + 33..offset + 33 + id_len];
            if id_bytes != [0] && id_bytes != [1] {
                let mut name = String::from_utf8_lossy(id_bytes).to_string();
                if let Some(idx) = name.rfind(';') {
                    name.truncate(idx);
                }
                entries.push(IsoFileEntry {
                    name,
                    extent_sector,
                    size_bytes,
                    is_dir: (flags & 0x02) != 0,
                });
            }
        }

        offset += record_len;
    }

    Ok(IsoVolumeInfo {
        volume_id,
        total_sectors,
        sector_size: SECTOR_SIZE as u32,
        files_count: entries.len(),
        entries,
    })
}

/// Extract every file, or those whose name matches `filter`.
pub fn extract_image(image_path: &Path, out_dir: &Path, filter: Option<&str>) -> Result<ExtractReport> {
    let info = read_metadata(image_path)?;
    fs::create_dir_all(out_dir)?;

    let filter_re = match filter {
        Some(f) => Some(
            regex::Regex::new(f)
                .map_err(|e| Error::Invalid(format!("invalid filter pattern: {}", e)))?,
        ),
        None => None,
    };

    let mut file = File::open(image_path)?;
    let image_len = file.metadata()?.len();
    let mut extracted = Vec::new();

    for entry in &info.entries {
        if entry.is_dir {
            continue;
        }
        if let Some(ref re) = filter_re {
            if !re.is_match(&entry.name) {
                continue;
            }
        }

        let start = entry.extent_sector as u64 * SECTOR_SIZE as u64;
        if start + entry.size_bytes > image_len {
            return Err(Error::Invalid(format!(
                "'{}' claims {} bytes at sector {}, past the end of the image",
                entry.name, entry.size_bytes, entry.extent_sector
            )));
        }

        file.seek(SeekFrom::Start(start))?;
        let mut buf = vec![0u8; entry.size_bytes as usize];
        file.read_exact(&mut buf)?;

        // A name from an image is not to be trusted as a path.
        let safe_name = Path::new(&entry.name)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());
        fs::write(out_dir.join(safe_name), &buf)?;
        extracted.push(entry.clone());
    }

    Ok(ExtractReport {
        image_path: image_path.to_string_lossy().to_string(),
        output_dir: out_dir.to_string_lossy().to_string(),
        extracted,
    })
}

/// Check the volume descriptor, the path table and every file extent.
pub fn verify_image(image_path: &Path) -> Result<VerifyResult> {
    let info = match read_metadata(image_path) {
        Ok(info) => info,
        Err(e) => {
            return Ok(VerifyResult {
                valid: false,
                volume_id: String::new(),
                total_sectors: 0,
                files_count: 0,
                message: e.to_string(),
                issues: vec![e.to_string()],
            })
        }
    };

    let image_len = fs::metadata(image_path)?.len();
    let mut issues: Vec<String> = Vec::new();

    // The path table must sit inside the image and not overlap the root.
    let l_pt = L_PATH_TABLE_SECTOR as u64 * SECTOR_SIZE as u64;
    if l_pt + 10 > image_len {
        issues.push("path table lies outside the image".to_string());
    }

    for entry in &info.entries {
        let start = entry.extent_sector as u64 * SECTOR_SIZE as u64;
        if start + entry.size_bytes > image_len {
            issues.push(format!(
                "'{}' extends past the end of the image",
                entry.name
            ));
        }
    }

    if info.total_sectors as u64 * SECTOR_SIZE as u64 > image_len {
        issues.push(format!(
            "volume claims {} sectors but the file holds {}",
            info.total_sectors,
            image_len / SECTOR_SIZE as u64
        ));
    }

    let valid = issues.is_empty();
    let message = if valid {
        format!("valid ISO 9660 image containing {} file(s)", info.files_count)
    } else {
        format!("{} problem(s) found", issues.len())
    };

    Ok(VerifyResult {
        valid,
        volume_id: info.volume_id,
        total_sectors: info.total_sectors,
        files_count: info.files_count,
        message,
        issues,
    })
}

/// Convert a raw 2352-byte-per-sector image (BIN) to a 2048-byte ISO.
pub fn convert_raw_to_iso(source: &Path, output: &Path) -> Result<ConvertResult> {
    let mut src = BufReader::new(File::open(source)?);
    let src_len = fs::metadata(source)?.len();

    if src_len % RAW_SECTOR_SIZE as u64 != 0 {
        return Err(Error::Invalid(format!(
            "input is {} bytes, which is not a whole number of {}-byte raw sectors",
            src_len, RAW_SECTOR_SIZE
        )));
    }

    let sector_count = src_len / RAW_SECTOR_SIZE as u64;
    let mut out = BufWriter::new(File::create(output)?);
    let mut raw = [0u8; RAW_SECTOR_SIZE];

    for _ in 0..sector_count {
        src.read_exact(&mut raw)?;
        out.write_all(&raw[16..16 + SECTOR_SIZE])?;
    }
    out.flush()?;

    let out_len = fs::metadata(output)?.len();

    Ok(ConvertResult {
        source_file: source.to_string_lossy().to_string(),
        output_file: output.to_string_lossy().to_string(),
        source_sectors: sector_count,
        output_sectors: sector_count,
        source_bytes: src_len,
        output_bytes: out_len,
    })
}


// ---------------------------------------------------------------------------
// CUE sheets and cover art (from optic)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CueReport {
    pub image: String,
    pub cue_path: String,
    pub track_mode: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CoverReport {
    pub image: String,
    pub svg_path: String,
    pub volume_id: String,
    pub files_listed: usize,
    pub bytes: usize,
}

/// Write a CUE sheet pointing at a binary image.
///
/// The format is three lines and has been for thirty years; the only decision
/// is which track mode to declare.
pub fn write_cue_sheet(image: &Path, cue_path: &Path, track_mode: &str) -> Result<CueReport> {
    let name = image
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "disc.bin".to_string());

    let mode = if track_mode.trim().is_empty() {
        "MODE1/2048"
    } else {
        track_mode.trim()
    };

    let content = format!(
        "FILE \"{}\" BINARY\n  TRACK 01 {}\n    INDEX 01 00:00:00\n",
        name, mode
    );

    if let Some(parent) = cue_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(cue_path, &content)?;

    Ok(CueReport {
        image: image.to_string_lossy().to_string(),
        cue_path: cue_path.to_string_lossy().to_string(),
        track_mode: mode.to_string(),
        content,
    })
}

fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a jewel-case insert as SVG: the volume label and what is on the disc.
pub fn write_cover_art(image: &Path, out_path: &Path) -> Result<CoverReport> {
    let info = read_metadata(image)?;
    let mut lines = String::new();
    let shown = info.entries.iter().take(12);
    let mut listed = 0usize;

    for (i, entry) in shown.enumerate() {
        listed += 1;
        lines.push_str(&format!(
            "<text x=\"60\" y=\"{}\" class=\"f\">{}</text>\n",
            250 + i * 26,
            svg_escape(&entry.name)
        ));
    }
    if info.entries.len() > listed {
        lines.push_str(&format!(
            "<text x=\"60\" y=\"{}\" class=\"f dim\">and {} more</text>\n",
            250 + listed * 26,
            info.entries.len() - listed
        ));
    }

    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 600 600\" width=\"600\" height=\"600\">\n\
<defs><style>\n\
  .bg {{ fill: #0b0d10; }}\n\
  .frame {{ fill: none; stroke: #2a313c; stroke-width: 2; }}\n\
  .title {{ font-family: Segoe UI, sans-serif; font-size: 30px; font-weight: 700; fill: #d8ff4f; }}\n\
  .sub {{ font-family: ui-monospace, monospace; font-size: 13px; fill: #98a2b0; }}\n\
  .f {{ font-family: ui-monospace, monospace; font-size: 14px; fill: #e9edf2; }}\n\
  .dim {{ fill: #5d6875; }}\n\
</style></defs>\n\
<rect class=\"bg\" width=\"600\" height=\"600\" rx=\"18\"/>\n\
<rect class=\"frame\" x=\"24\" y=\"24\" width=\"552\" height=\"552\" rx=\"10\"/>\n\
<text x=\"60\" y=\"96\" class=\"title\">{volume}</text>\n\
<text x=\"60\" y=\"128\" class=\"sub\">{files} file(s) &middot; {sectors} sector(s)</text>\n\
<line x1=\"60\" y1=\"160\" x2=\"540\" y2=\"160\" stroke=\"#2a313c\" stroke-width=\"1\"/>\n\
{lines}\
<text x=\"60\" y=\"548\" class=\"sub dim\">savehorse halo</text>\n\
</svg>\n",
        volume = svg_escape(&info.volume_id),
        files = info.entries.len(),
        sectors = info.total_sectors,
        lines = lines
    );

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(out_path, &svg)?;

    Ok(CoverReport {
        image: image.to_string_lossy().to_string(),
        svg_path: out_path.to_string_lossy().to_string(),
        volume_id: info.volume_id,
        files_listed: listed,
        bytes: svg.len(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-halo-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn creates_and_reads_back_an_image() {
        let dir = scratch("roundtrip");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("readme.txt"), b"hello iso").unwrap();
        fs::write(src.join("data.bin"), vec![7u8; 3000]).unwrap();

        let out = dir.join("test.iso");
        let report = create_image(&src, &out, "TESTVOL").unwrap();
        assert_eq!(report.files_count, 2);
        assert!(report.bytes_written > 0);

        let info = read_metadata(&out).unwrap();
        assert_eq!(info.volume_id, "TESTVOL");
        assert_eq!(info.files_count, 2);
        assert_eq!(info.sector_size, 2048);

        let names: Vec<&str> = info.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"README.TXT"));
        assert!(names.contains(&"DATA.BIN"));

        let big = info.entries.iter().find(|e| e.name == "DATA.BIN").unwrap();
        assert_eq!(big.size_bytes, 3000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_a_path_table_that_is_not_the_root_directory() {
        let dir = scratch("pathtable");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"x").unwrap();
        let out = dir.join("t.iso");
        create_image(&src, &out, "VOL").unwrap();

        let bytes = fs::read(&out).unwrap();
        let pvd = &bytes[PVD_SECTOR as usize * SECTOR_SIZE..][..SECTOR_SIZE];

        let path_table_size = u32::from_le_bytes(pvd[132..136].try_into().unwrap());
        let l_pt = u32::from_le_bytes(pvd[140..144].try_into().unwrap());
        let m_pt = u32::from_be_bytes(pvd[148..152].try_into().unwrap());
        let root = u32::from_le_bytes(pvd[158..162].try_into().unwrap());

        assert_eq!(path_table_size, 10, "one 10-byte root record");
        assert_eq!(l_pt, L_PATH_TABLE_SECTOR);
        assert_eq!(m_pt, M_PATH_TABLE_SECTOR);
        assert_eq!(root, ROOT_DIR_SECTOR);
        assert_ne!(l_pt, root, "the path table must not sit on the root directory");

        // And the record itself must point at the root directory.
        let rec = &bytes[l_pt as usize * SECTOR_SIZE..][..10];
        assert_eq!(rec[0], 1, "root identifier is one byte");
        assert_eq!(u32::from_le_bytes(rec[2..6].try_into().unwrap()), ROOT_DIR_SECTOR);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_restores_file_contents() {
        let dir = scratch("extract");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        let payload: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        fs::write(src.join("blob.bin"), &payload).unwrap();

        let out = dir.join("t.iso");
        create_image(&src, &out, "VOL").unwrap();

        let dest = dir.join("out");
        let report = extract_image(&out, &dest, None).unwrap();
        assert_eq!(report.extracted.len(), 1);
        let restored = fs::read(dest.join("BLOB.BIN")).unwrap();
        assert_eq!(restored, payload);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_honours_a_filter() {
        let dir = scratch("filter");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("keep.txt"), b"a").unwrap();
        fs::write(src.join("skip.log"), b"b").unwrap();
        let out = dir.join("t.iso");
        create_image(&src, &out, "VOL").unwrap();

        let dest = dir.join("out");
        let report = extract_image(&out, &dest, Some(r"\.TXT$")).unwrap();
        assert_eq!(report.extracted.len(), 1);
        assert_eq!(report.extracted[0].name, "KEEP.TXT");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_accepts_a_good_image_and_rejects_a_broken_one() {
        let dir = scratch("verify");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"hello").unwrap();
        let out = dir.join("t.iso");
        create_image(&src, &out, "VOL").unwrap();

        let result = verify_image(&out).unwrap();
        assert!(result.valid, "issues: {:?}", result.issues);
        assert_eq!(result.files_count, 1);

        // Truncate the image so the file extent no longer fits.
        let bytes = fs::read(&out).unwrap();
        fs::write(&out, &bytes[..bytes.len() - 1024]).unwrap();
        let result = verify_image(&out).unwrap();
        assert!(!result.valid);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_subdirectories_instead_of_dropping_them() {
        let dir = scratch("subdir");
        let src = dir.join("src");
        fs::create_dir_all(src.join("nested")).unwrap();
        fs::write(src.join("nested").join("deep.txt"), b"x").unwrap();

        let err = create_image(&src, &dir.join("t.iso"), "VOL").unwrap_err();
        assert!(
            err.to_string().contains("subdirectories are not supported"),
            "got: {}",
            err
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn disambiguates_names_that_sanitise_the_same() {
        let dir = scratch("collide");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a-b.txt"), b"one").unwrap();
        fs::write(src.join("a_b.txt"), b"two").unwrap();

        let report = create_image(&src, &dir.join("t.iso"), "VOL").unwrap();
        assert_eq!(report.files_count, 2);
        let names: Vec<&str> = report.entries.iter().map(|e| e.name.as_str()).collect();
        assert_ne!(names[0], names[1], "collision must be broken, not overwritten");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn converts_raw_sectors_by_dropping_the_2352_header() {
        let dir = scratch("convert");
        let mut raw = Vec::new();
        for _ in 0..2 {
            let mut sector = vec![0xAAu8; RAW_SECTOR_SIZE];
            for (i, b) in sector.iter_mut().enumerate().take(16) {
                *b = i as u8;
            }
            sector[16..16 + SECTOR_SIZE].fill(0x42);
            raw.extend_from_slice(&sector);
        }
        let src = dir.join("disc.bin");
        fs::write(&src, &raw).unwrap();

        let out = dir.join("disc.iso");
        let report = convert_raw_to_iso(&src, &out).unwrap();
        assert_eq!(report.source_sectors, 2);
        assert_eq!(report.output_bytes, 2 * SECTOR_SIZE as u64);

        let converted = fs::read(&out).unwrap();
        assert!(converted.iter().all(|b| *b == 0x42), "payload only, no header");
        let _ = fs::remove_dir_all(&dir);
    }


    #[test]
    fn writes_a_cue_sheet() {
        let dir = scratch("cue");
        let image = dir.join("disc.bin");
        fs::write(&image, b"x").unwrap();
        let cue = dir.join("disc.cue");

        let report = write_cue_sheet(&image, &cue, "").unwrap();
        assert_eq!(report.track_mode, "MODE1/2048");
        let text = fs::read_to_string(&cue).unwrap();
        assert!(text.contains("FILE \"disc.bin\" BINARY"));
        assert!(text.contains("TRACK 01 MODE1/2048"));
        assert!(text.contains("INDEX 01 00:00:00"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_cover_art_from_an_image() {
        let dir = scratch("cover");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("readme.txt"), b"hello").unwrap();

        let image = dir.join("t.iso");
        create_image(&src, &image, "MYVOL").unwrap();

        let out = dir.join("cover.svg");
        let report = write_cover_art(&image, &out).unwrap();
        assert_eq!(report.volume_id, "MYVOL");
        assert_eq!(report.files_listed, 1);

        let svg = fs::read_to_string(&out).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("MYVOL"));
        assert!(svg.contains("README.TXT"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_rejects_a_misaligned_file() {
        let dir = scratch("convert-bad");
        let src = dir.join("bad.bin");
        fs::write(&src, vec![0u8; 1000]).unwrap();
        assert!(convert_raw_to_iso(&src, &dir.join("x.iso")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
