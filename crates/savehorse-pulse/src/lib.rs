//! pulse — the system module.
//!
//! From `pulse` 1.0.0 (generation 2) and `beacon` 1.0.0 (generation 1).
//!
//! The two operations here need nothing from the platform: a file hash, and an
//! audit of how much space the temporary directory is holding. They are the
//! portable core of the module.
//!
//! The rest of `pulse` — hardware, processes, drives, services, priorities —
//! needs a system-information crate, and the original reached straight into
//! Win32 for all of it. Those stay declared-but-planned until they are ported
//! against a cross-platform API, which is a separate piece of work and is
//! recorded as such in the capability matrix.

use savehorse_core::{crc32, format_bytes, sha256_file, Error, Result, WalkOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct HashReport {
    pub file: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub crc32: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TempEntry {
    pub path: String,
    pub size_bytes: u64,
    pub size_human: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TempReport {
    pub directory: String,
    pub files: usize,
    pub total_bytes: u64,
    pub total_human: String,
    /// The largest files, biggest first — what is actually worth clearing.
    pub largest: Vec<TempEntry>,
}

/// CRC-32 and SHA-256 of a file, which is what you need to compare two copies
/// or to check one against a published value.
pub fn hash_file(path: &Path) -> Result<HashReport> {
    let data = std::fs::read(path)?;
    let size = data.len() as u64;

    Ok(HashReport {
        file: path.to_string_lossy().to_string(),
        size_bytes: size,
        size_human: format_bytes(size),
        crc32: format!("{:08x}", crc32(&data)),
        sha256: sha256_file(path)?,
    })
}

/// Measure what a directory is holding, without deleting anything.
///
/// The default target is the operating system's temporary directory. Reading
/// the tree is a scan, not a cleanup: nothing here removes a file.
pub fn audit_temp(directory: Option<&Path>, top: usize) -> Result<TempReport> {
    let root: PathBuf = match directory {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir(),
    };

    if !root.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            root.display()
        )));
    }

    let files = savehorse_core::walk_files(
        &root,
        &WalkOptions {
            include_hidden: true,
            ..Default::default()
        },
    )?;

    let mut total = 0u64;
    let mut entries: Vec<TempEntry> = Vec::with_capacity(files.len());

    for file in &files {
        let size = match std::fs::metadata(file) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        total += size;
        entries.push(TempEntry {
            path: file.to_string_lossy().to_string(),
            size_bytes: size,
            size_human: format_bytes(size),
        });
    }

    entries.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    entries.truncate(top.max(1));

    Ok(TempReport {
        directory: root.to_string_lossy().to_string(),
        files: files.len(),
        total_bytes: total,
        total_human: format_bytes(total),
        largest: entries,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-pulse-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn hashes_a_file_both_ways() {
        let dir = scratch("hash");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();

        let report = hash_file(&file).unwrap();
        assert_eq!(report.size_bytes, 3);
        assert_eq!(
            report.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // The IEEE CRC-32 of "abc" is 0x352441C2.
        assert_eq!(report.crc32, "352441c2");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_counts_and_ranks_without_deleting() {
        let dir = scratch("temp");
        fs::write(dir.join("small.txt"), b"x").unwrap();
        fs::write(dir.join("big.txt"), vec![0u8; 2048]).unwrap();
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("mid.txt"), vec![0u8; 100]).unwrap();

        let report = audit_temp(Some(&dir), 2).unwrap();
        assert_eq!(report.files, 3);
        assert_eq!(report.total_bytes, 2149);
        assert_eq!(report.largest.len(), 2, "top two only");
        assert!(report.largest[0].path.ends_with("big.txt"), "biggest first");

        // An audit is a read: everything is still there.
        assert!(dir.join("big.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_rejects_a_path_that_is_not_a_directory() {
        let dir = scratch("temp-bad");
        let file = dir.join("a.txt");
        fs::write(&file, b"x").unwrap();
        assert!(audit_temp(Some(&file), 5).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
