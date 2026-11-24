//! forge — the binary lab.
//!
//! Ported from `forge` 1.0.0 (generation 1). The PE parsing is the original's
//! hand-rolled reader — no dependency, no framework, and it only needs the
//! handful of header fields it actually reports.
//!
//! Three things were tightened:
//!
//! 1. `patch` can now preview. The original wrote bytes the moment it was
//!    called; a preview reports the old and new bytes and touches nothing.
//! 2. `patch` refuses an offset that runs past the end of the file instead of
//!    failing halfway through a write.
//! 3. `inspect` no longer reads a fixed 72 bytes when the optional header
//!    claims to be empty, which could quietly read into the section table.
//!
//! `imports` is a heuristic, not a real import-table walk: it scans for
//! printable strings ending in `.dll`. That is what the original did, and it
//! is described that way rather than as something it is not.

use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PeHeaderInfo {
    pub file_path: String,
    pub is_valid_pe: bool,
    pub machine_type: String,
    pub is_64bit: bool,
    pub number_of_sections: u16,
    pub entry_point_rva: u32,
    pub image_base: u64,
    pub subsystem_id: u16,
    pub subsystem_name: String,
    pub file_size_bytes: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SectionInfo {
    pub name: String,
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub raw_data_size: u32,
    pub raw_data_offset: u32,
    pub entropy: f64,
    pub is_packed: bool,
    pub characteristics: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HexLine {
    pub offset: usize,
    pub hex_bytes: String,
    pub ascii: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SecurityFlags {
    pub file_path: String,
    pub is_64bit: bool,
    pub machine_type: String,
    /// Data execution prevention (NX_COMPAT, 0x0100).
    pub dep: bool,
    /// Address space layout randomisation (DYNAMIC_BASE, 0x0040).
    pub aslr: bool,
    /// High-entropy 64-bit ASLR (HIGH_ENTROPY_VA, 0x0020).
    pub high_entropy_aslr: bool,
    /// Control Flow Guard (GUARD_CF, 0x4000).
    pub control_flow_guard: bool,
    /// A non-zero certificate table, which usually means Authenticode signed.
    pub signed_heuristic: bool,
    /// Raw DllCharacteristics for anything not covered above.
    pub dll_characteristics: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PatchResult {
    pub file: String,
    pub offset: u64,
    pub bytes_patched: usize,
    pub original_bytes: String,
    pub new_bytes: String,
    pub backup_created: Option<String>,
    pub written: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Shannon entropy in bits per byte, 0.0 to 8.0.
pub fn calculate_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let length = data.len() as f64;
    let mut entropy = 0.0f64;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / length;
            entropy -= p * p.log2();
        }
    }
    (entropy * 10000.0).round() / 10000.0
}

fn read_u16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn read_u32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn hex_of(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a hex string such as "4D 5A" or "4d5a" into bytes.
fn parse_hex(text: &str) -> Result<Vec<u8>> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.is_empty() {
        return Err(Error::Invalid("no bytes given".to_string()));
    }
    if clean.len() % 2 != 0 {
        return Err(Error::Invalid(
            "a hex string needs an even number of characters".to_string(),
        ));
    }

    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2])
            .map_err(|_| Error::Invalid("hex string is not valid text".to_string()))?;
        let value = u8::from_str_radix(pair, 16)
            .map_err(|_| Error::Invalid(format!("'{}' is not a hex byte", pair)))?;
        out.push(value);
        i += 2;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// PE headers
// ---------------------------------------------------------------------------

/// Read the DOS, COFF and optional headers of a PE executable.
pub fn inspect_pe(path: &Path) -> Result<PeHeaderInfo> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size < 64 {
        return Err(Error::Invalid(
            "file is too small to be a PE executable".to_string(),
        ));
    }

    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[0..2] != b"MZ" {
        return Err(Error::Invalid(
            "not a PE binary: the MZ signature is missing".to_string(),
        ));
    }

    let e_lfanew = read_u32(&dos, 60) as u64;
    if e_lfanew + 24 > file_size {
        return Err(Error::Invalid(
            "the DOS header points past the end of the file".to_string(),
        ));
    }

    file.seek(SeekFrom::Start(e_lfanew))?;
    let mut coff = [0u8; 24];
    file.read_exact(&mut coff)?;
    if &coff[0..4] != b"PE\0\0" {
        return Err(Error::Invalid(
            "not a PE binary: the PE signature is missing".to_string(),
        ));
    }

    let machine = read_u16(&coff, 4);
    let num_sections = read_u16(&coff, 6);
    let opt_size = read_u16(&coff, 20);

    if opt_size < 72 {
        return Err(Error::Invalid(
            "the optional header is too small to describe the image".to_string(),
        ));
    }

    let (machine_type, is_64bit) = match machine {
        0x8664 => ("x86_64 (AMD64)".to_string(), true),
        0x014C => ("x86 (32-bit)".to_string(), false),
        0xAA64 => ("ARM64".to_string(), true),
        0x01C0 => ("ARM".to_string(), false),
        other => (format!("unknown (0x{:04X})", other), false),
    };

    let mut opt = vec![0u8; opt_size as usize];
    file.read_exact(&mut opt)?;

    let magic = read_u16(&opt, 0);
    let entry_point = read_u32(&opt, 16);
    let subsystem = read_u16(&opt, 68);

    let image_base = if magic == 0x020B {
        u64::from_le_bytes([
            opt[24], opt[25], opt[26], opt[27], opt[28], opt[29], opt[30], opt[31],
        ])
    } else {
        read_u32(&opt, 28) as u64
    };

    let subsystem_name = match subsystem {
        1 => "native driver",
        2 => "Windows GUI",
        3 => "Windows console",
        9 => "Windows CE GUI",
        10 => "EFI application",
        _ => "other",
    };

    Ok(PeHeaderInfo {
        file_path: savehorse_core::display_path(&path),
        is_valid_pe: true,
        machine_type,
        is_64bit,
        number_of_sections: num_sections,
        entry_point_rva: entry_point,
        image_base,
        subsystem_id: subsystem,
        subsystem_name: subsystem_name.to_string(),
        file_size_bytes: file_size,
    })
}

/// List the section table, with each section's entropy.
pub fn inspect_sections(path: &Path) -> Result<Vec<SectionInfo>> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)?;
    let e_lfanew = read_u32(&dos, 60) as u64;

    file.seek(SeekFrom::Start(e_lfanew + 4))?;
    let mut coff = [0u8; 20];
    file.read_exact(&mut coff)?;

    let num_sections = read_u16(&coff, 2) as usize;
    let opt_size = read_u16(&coff, 16) as u64;
    let table_offset = e_lfanew + 24 + opt_size;

    if table_offset + (num_sections as u64 * 40) > file_size {
        return Err(Error::Invalid(
            "the section table runs past the end of the file".to_string(),
        ));
    }

    file.seek(SeekFrom::Start(table_offset))?;

    let mut sections = Vec::with_capacity(num_sections);

    for _ in 0..num_sections {
        let mut header = [0u8; 40];
        file.read_exact(&mut header)?;

        let name_bytes = &header[0..8];
        let name_end = name_bytes.iter().position(|&c| c == 0).unwrap_or(8);
        let name = String::from_utf8_lossy(&name_bytes[..name_end]).to_string();

        let virtual_size = read_u32(&header, 8);
        let virtual_address = read_u32(&header, 12);
        let raw_size = read_u32(&header, 16);
        let raw_offset = read_u32(&header, 20);
        let characteristics = read_u32(&header, 36);

        let mut flags = Vec::new();
        if characteristics & 0x2000_0000 != 0 {
            flags.push("EXEC");
        }
        if characteristics & 0x4000_0000 != 0 {
            flags.push("READ");
        }
        if characteristics & 0x8000_0000 != 0 {
            flags.push("WRITE");
        }

        sections.push((name, virtual_size, virtual_address, raw_size, raw_offset, flags.join("|")));
    }

    let mut out = Vec::with_capacity(sections.len());
    for (name, virtual_size, virtual_address, raw_size, raw_offset, flags) in sections {
        let entropy = if raw_size > 0 && (raw_offset as u64) < file_size {
            let available = (file_size - raw_offset as u64).min(raw_size as u64) as usize;
            let take = available.min(1024 * 1024);
            file.seek(SeekFrom::Start(raw_offset as u64))?;
            let mut buf = vec![0u8; take];
            let read = file.read(&mut buf)?;
            calculate_entropy(&buf[..read])
        } else {
            0.0
        };

        out.push(SectionInfo {
            name,
            virtual_size,
            virtual_address,
            raw_data_size: raw_size,
            raw_data_offset: raw_offset,
            entropy,
            // Above 7.2 bits per byte there is almost no structure left, which
            // is the signature of a packed or encrypted section.
            is_packed: entropy > 7.2,
            characteristics: flags,
        });
    }

    Ok(out)
}

/// Find printable strings ending in `.dll`.
pub fn extract_imports(path: &Path) -> Result<Vec<String>> {
    let data = fs::read(path)?;
    let mut found: Vec<String> = Vec::new();
    let mut i = 0usize;

    while i + 4 <= data.len() {
        if data[i..i + 4].eq_ignore_ascii_case(b".dll") {
            let mut start = i;
            while start > 0
                && data[start - 1] >= 0x20
                && data[start - 1] <= 0x7E
                && data[start - 1] != b' '
            {
                start -= 1;
            }
            if start < i {
                let name = String::from_utf8_lossy(&data[start..i + 4]).to_lowercase();
                if name.len() > 4 && !found.contains(&name) {
                    found.push(name);
                }
            }
            i += 4;
        } else {
            i += 1;
        }
    }

    found.sort();
    Ok(found)
}

// ---------------------------------------------------------------------------
// Hex
// ---------------------------------------------------------------------------

/// Classic hex and ASCII view, sixteen bytes to a line.
pub fn format_hexdump(data: &[u8], start_offset: usize) -> Vec<HexLine> {
    let mut lines = Vec::with_capacity(data.len() / 16 + 1);
    let mut offset = 0usize;

    while offset < data.len() {
        let take = (data.len() - offset).min(16);
        let chunk = &data[offset..offset + take];

        let mut parts: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
        while parts.len() < 16 {
            parts.push("  ".to_string());
        }

        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..=0x7E).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();

        lines.push(HexLine {
            offset: start_offset + offset,
            hex_bytes: format!("{}  {}", parts[0..8].join(" "), parts[8..16].join(" ")),
            ascii,
        });

        offset += 16;
    }

    lines
}

/// Dump part of a file.
pub fn hexdump_file(path: &Path, offset: u64, length: usize) -> Result<Vec<HexLine>> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();

    if offset > size {
        return Err(Error::Invalid(format!(
            "offset {} is past the end of the file ({} bytes)",
            offset, size
        )));
    }

    file.seek(SeekFrom::Start(offset))?;
    let available = (size - offset) as usize;
    let take = length.min(available);
    let mut buffer = vec![0u8; take];
    file.read_exact(&mut buffer)?;

    Ok(format_hexdump(&buffer, offset as usize))
}

/// Find every occurrence of a hex byte sequence.
pub fn search_hex_pattern(path: &Path, pattern: &str) -> Result<Vec<u64>> {
    let needle = parse_hex(pattern)?;
    let data = fs::read(path)?;

    let mut hits = Vec::new();
    if needle.len() > data.len() {
        return Ok(hits);
    }
    for i in 0..=(data.len() - needle.len()) {
        if &data[i..i + needle.len()] == &needle[..] {
            hits.push(i as u64);
        }
    }
    Ok(hits)
}

/// Overwrite bytes at an offset, optionally after making a `.bak` copy.
///
/// With `write` false this reports what would change and leaves the file
/// alone.
pub fn apply_patch(
    path: &Path,
    offset: u64,
    replacement: &str,
    create_backup: bool,
    write: bool,
) -> Result<PatchResult> {
    let bytes = parse_hex(replacement)?;
    let size = fs::metadata(path)?.len();

    if offset + bytes.len() as u64 > size {
        return Err(Error::Invalid(format!(
            "{} byte(s) at offset {} would run past the end of the file ({} bytes)",
            bytes.len(),
            offset,
            size
        )));
    }

    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut original = vec![0u8; bytes.len()];
    file.read_exact(&mut original)?;

    let backup_created = if write && create_backup {
        let bak = format!("{}.bak", savehorse_core::display_path(&path));
        fs::copy(path, &bak)?;
        Some(bak)
    } else {
        None
    };

    if write {
        let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&bytes)?;
        file.flush()?;
    }

    Ok(PatchResult {
        file: savehorse_core::display_path(&path),
        offset,
        bytes_patched: bytes.len(),
        original_bytes: hex_of(&original),
        new_bytes: hex_of(&bytes),
        backup_created,
        written: write,
    })
}


/// Read the hardening flags a PE sets about itself.
///
/// These are the bits the loader obeys: whether the image opts into DEP, into
/// ASLR, and whether it was built with Control Flow Guard. `signed_heuristic`
/// only checks that the certificate table is non-empty — it does not validate
/// the signature.
pub fn security_flags(path: &Path) -> Result<SecurityFlags> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[0..2] != b"MZ" {
        return Err(Error::Invalid(
            "not a PE binary: the MZ signature is missing".to_string(),
        ));
    }

    let e_lfanew = read_u32(&dos, 60) as u64;
    if e_lfanew + 24 > file_size {
        return Err(Error::Invalid(
            "the DOS header points past the end of the file".to_string(),
        ));
    }

    file.seek(SeekFrom::Start(e_lfanew))?;
    let mut coff = [0u8; 24];
    file.read_exact(&mut coff)?;
    if &coff[0..4] != b"PE\0\0" {
        return Err(Error::Invalid(
            "not a PE binary: the PE signature is missing".to_string(),
        ));
    }

    let machine = read_u16(&coff, 4);
    let opt_size = read_u16(&coff, 20);
    if opt_size < 70 {
        return Err(Error::Invalid(
            "the optional header is too small to describe the image".to_string(),
        ));
    }

    let mut opt = vec![0u8; opt_size as usize];
    file.read_exact(&mut opt)?;

    let magic = read_u16(&opt, 0);
    let is_64bit = magic == 0x020B;
    let characteristics = read_u16(&opt, 70);

    // The certificate table is data directory entry 4. The array starts at 112
    // for PE32+ and 96 for PE32, and each entry is eight bytes.
    let dir_base = if is_64bit { 112 } else { 96 };
    let cert_offset = dir_base + 4 * 8;
    let signed = if opt.len() >= cert_offset + 8 {
        read_u32(&opt, cert_offset) != 0 && read_u32(&opt, cert_offset + 4) != 0
    } else {
        false
    };

    let machine_type = match machine {
        0x8664 => "x86_64 (AMD64)",
        0x014C => "x86 (32-bit)",
        0xAA64 => "ARM64",
        0x01C0 => "ARM",
        _ => "unknown",
    };

    Ok(SecurityFlags {
        file_path: savehorse_core::display_path(&path),
        is_64bit,
        machine_type: machine_type.to_string(),
        dep: characteristics & 0x0100 != 0,
        aslr: characteristics & 0x0040 != 0,
        high_entropy_aslr: characteristics & 0x0020 != 0,
        control_flow_guard: characteristics & 0x4000 != 0,
        signed_heuristic: signed,
        dll_characteristics: characteristics,
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
        p.push(format!("savehorse-forge-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// A minimal but structurally valid PE32 file, enough for the header reader.
    fn minimal_pe() -> Vec<u8> {
        let mut pe = vec![0u8; 0x300];

        // DOS header
        pe[0] = b'M';
        pe[1] = b'Z';
        pe[60..64].copy_from_slice(&0x80u32.to_le_bytes()); // e_lfanew

        // PE signature at 0x80
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");

        // COFF header
        pe[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes()); // machine
        pe[0x86..0x88].copy_from_slice(&1u16.to_le_bytes()); // sections
        pe[0x94..0x96].copy_from_slice(&0xF0u16.to_le_bytes()); // optional header size

        // Optional header (PE32+)
        let opt = 0x98;
        pe[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes()); // magic
        pe[opt + 16..opt + 20].copy_from_slice(&0x1000u32.to_le_bytes()); // entry point
        pe[opt + 24..opt + 32].copy_from_slice(&0x140000000u64.to_le_bytes()); // image base
        pe[opt + 68..opt + 70].copy_from_slice(&3u16.to_le_bytes()); // console subsystem

        // One section header at 0x98 + 0xF0
        let sec = 0x98 + 0xF0;
        pe[sec..sec + 5].copy_from_slice(b".text");
        pe[sec + 8..sec + 12].copy_from_slice(&0x100u32.to_le_bytes()); // virtual size
        pe[sec + 12..sec + 16].copy_from_slice(&0x1000u32.to_le_bytes()); // virtual address
        pe[sec + 16..sec + 20].copy_from_slice(&0x100u32.to_le_bytes()); // raw size
        // Raw data must sit past the headers: the section table ends at
        // 0x188, and overlapping it makes the optional header read as garbage.
        pe[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes()); // raw offset
        pe[sec + 36..sec + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes()); // code|exec|read

        // Some bytes in the section body, past the header block.
        for (i, b) in pe[0x200..0x300].iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        pe
    }

    #[test]
    fn reads_hardening_flags() {
        let dir = scratch("flags");
        let file = dir.join("a.exe");
        let mut pe = minimal_pe();
        // DEP (0x0100) | ASLR (0x0040) | HIGH_ENTROPY_VA (0x0020) | CFG (0x4000)
        pe[0x98 + 70..0x98 + 72].copy_from_slice(&0x4160u16.to_le_bytes());
        fs::write(&file, &pe).unwrap();

        let flags = security_flags(&file).unwrap();
        assert!(flags.is_64bit);
        assert!(flags.dep, "NX_COMPAT should be set");
        assert!(flags.aslr, "DYNAMIC_BASE should be set");
        assert!(flags.high_entropy_aslr);
        assert!(flags.control_flow_guard);
        assert!(!flags.signed_heuristic, "no certificate table in the fixture");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_a_pe_header() {
        let dir = scratch("pe");
        let file = dir.join("a.exe");
        fs::write(&file, minimal_pe()).unwrap();

        let info = inspect_pe(&file).unwrap();
        assert!(info.is_valid_pe);
        assert!(info.is_64bit);
        assert_eq!(info.number_of_sections, 1);
        assert_eq!(info.entry_point_rva, 0x1000);
        assert_eq!(info.image_base, 0x140000000);
        assert_eq!(info.subsystem_name, "Windows console");
        assert!(info.machine_type.contains("x86_64"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_a_file_that_is_not_pe() {
        let dir = scratch("notpe");
        let file = dir.join("a.txt");
        // Long enough to pass the 64-byte minimum, so the MZ check is the
        // thing being tested rather than the size guard.
        fs::write(
            &file,
            b"this is not an executable at all, not even close, not by any measure whatsoever",
        )
        .unwrap();
        let err = inspect_pe(&file).unwrap_err().to_string();
        assert!(err.contains("MZ"), "got: {}", err);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_sections_with_flags_and_entropy() {
        let dir = scratch("sections");
        let file = dir.join("a.exe");
        fs::write(&file, minimal_pe()).unwrap();

        let sections = inspect_sections(&file).unwrap();
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, ".text");
        assert_eq!(sections[0].raw_data_size, 0x100);
        assert!(sections[0].characteristics.contains("EXEC"));
        assert!(sections[0].characteristics.contains("READ"));
        assert!(sections[0].entropy > 0.0 && sections[0].entropy <= 8.0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn entropy_is_zero_for_uniform_data_and_maximal_for_noise() {
        assert_eq!(calculate_entropy(&[0u8; 1000]), 0.0);
        let all: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let e = calculate_entropy(&all);
        assert!((e - 8.0).abs() < 0.01, "expected ~8.0, got {}", e);
    }

    #[test]
    fn hexdump_shows_sixteen_bytes_a_line() {
        let data: Vec<u8> = (0u8..20).collect();
        let lines = format_hexdump(&data, 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].offset, 0);
        assert!(lines[0].hex_bytes.starts_with("00 01 02 03"));
        assert_eq!(lines[1].offset, 16);
        assert_eq!(lines[1].ascii.len(), 4);
    }

    #[test]
    fn hexdump_reads_a_window_of_a_file() {
        let dir = scratch("hexdump");
        let file = dir.join("a.bin");
        fs::write(&file, (0u8..64).collect::<Vec<u8>>()).unwrap();

        let lines = hexdump_file(&file, 16, 16).unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].offset, 16);
        assert!(lines[0].hex_bytes.starts_with("10 11 12"));

        assert!(hexdump_file(&file, 999, 16).is_err(), "offset past the end");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn searches_for_a_hex_pattern() {
        let dir = scratch("search");
        let file = dir.join("a.bin");
        fs::write(&file, b"\x00\x01\xDE\xAD\xBE\xEF\x00\xDE\xAD\xBE\xEF").unwrap();

        let hits = search_hex_pattern(&file, "DE AD BE EF").unwrap();
        assert_eq!(hits, vec![2, 7]);

        let compact = search_hex_pattern(&file, "deadbeef").unwrap();
        assert_eq!(compact, vec![2, 7]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_rejects_a_malformed_pattern() {
        let dir = scratch("search-bad");
        let file = dir.join("a.bin");
        fs::write(&file, b"data").unwrap();
        assert!(search_hex_pattern(&file, "DEA").is_err(), "odd length");
        assert!(search_hex_pattern(&file, "ZZ").is_err(), "not hex");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_previews_without_writing() {
        let dir = scratch("patch-preview");
        let file = dir.join("a.bin");
        fs::write(&file, b"AAAAAAAAAA").unwrap();

        let result = apply_patch(&file, 2, "42 43", false, false).unwrap();
        assert!(!result.written);
        assert_eq!(result.original_bytes, "41 41");
        assert_eq!(result.new_bytes, "42 43");
        assert_eq!(fs::read(&file).unwrap(), b"AAAAAAAAAA");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_writes_and_keeps_a_backup() {
        let dir = scratch("patch-write");
        let file = dir.join("a.bin");
        fs::write(&file, b"AAAAAAAAAA").unwrap();

        let result = apply_patch(&file, 2, "42 43", true, true).unwrap();
        assert!(result.written);
        assert!(result.backup_created.is_some());
        assert_eq!(fs::read(&file).unwrap(), b"AA\x42\x43AAAAAA");
        assert_eq!(fs::read(dir.join("a.bin.bak")).unwrap(), b"AAAAAAAAAA");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_refuses_to_run_past_the_end() {
        let dir = scratch("patch-oob");
        let file = dir.join("a.bin");
        fs::write(&file, b"1234").unwrap();
        let err = apply_patch(&file, 3, "41 42 43", false, true).unwrap_err();
        assert!(err.to_string().contains("past the end"), "got: {}", err);
        assert_eq!(fs::read(&file).unwrap(), b"1234", "file must be untouched");
        let _ = fs::remove_dir_all(&dir);
    }
}
