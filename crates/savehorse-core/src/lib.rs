//! Shared foundation for every savehorse module.
//!
//! Nothing in here knows about a command line, a window, or a particular
//! module. Everything returns data or an [`Error`]; printing is the caller's
//! job. That rule is what lets one implementation serve the CLI, the desktop
//! app and the recipe runner.

use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// The single error type used across the workspace.
#[derive(Debug)]
pub enum Error {
    /// Wraps a standard library IO failure.
    Io(std::io::Error),
    /// The caller passed something that cannot work.
    Invalid(String),
    /// A path that should exist does not.
    NotFound(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{}", e),
            Error::Invalid(m) => write!(f, "{}", m),
            Error::NotFound(p) => write!(f, "no such file or directory: {}", p),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Invalid(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Invalid(s.to_string())
    }
}

/// Shorthand used by every module.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Byte sizes
// ---------------------------------------------------------------------------

/// Render a byte count the way a person reads it: `1.50 MB`, `812 B`.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Parse `10`, `10b`, `10k`, `10m`, `10g` into a byte count.
pub fn parse_size(s: &str) -> Result<u64> {
    let t = s.trim().to_ascii_lowercase();
    let (num, mult) = match t.chars().last() {
        Some('k') => (&t[..t.len() - 1], 1024u64),
        Some('m') => (&t[..t.len() - 1], 1024 * 1024),
        Some('g') => (&t[..t.len() - 1], 1024 * 1024 * 1024),
        Some('b') => (&t[..t.len() - 1], 1),
        _ => (t.as_str(), 1),
    };
    let n: u64 = num
        .trim()
        .parse()
        .map_err(|_| Error::Invalid(format!("not a size: '{}'", s)))?;
    Ok(n.saturating_mul(mult))
}

// ---------------------------------------------------------------------------
// Hashing
// ---------------------------------------------------------------------------

/// Lowercase hex for a byte slice.
pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

/// CRC-32 (IEEE 802.3), the same polynomial the original tools used.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Streaming SHA-256 of a file, returned as lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

// ---------------------------------------------------------------------------
// Directory walking
// ---------------------------------------------------------------------------

/// How deep and what to keep while walking a tree.
#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// `None` walks the whole tree. `Some(0)` lists only the root's files.
    pub max_depth: Option<usize>,
    /// Skip files smaller than this.
    pub min_size: Option<u64>,
    /// Skip files larger than this.
    pub max_size: Option<u64>,
    /// Keep only this extension, with or without the leading dot.
    pub ext: Option<String>,
    /// Include dot-files and dot-directories. Off by default.
    pub include_hidden: bool,
}

impl Default for WalkOptions {
    fn default() -> Self {
        WalkOptions {
            max_depth: None,
            min_size: None,
            max_size: None,
            ext: None,
            include_hidden: false,
        }
    }
}

fn size_allowed(size: u64, opts: &WalkOptions) -> bool {
    if let Some(min) = opts.min_size {
        if size < min {
            return false;
        }
    }
    if let Some(max) = opts.max_size {
        if size > max {
            return false;
        }
    }
    true
}

fn ext_allowed(path: &Path, opts: &WalkOptions) -> bool {
    match opts.ext {
        None => true,
        Some(ref want) => {
            let want = want.trim_start_matches('.').to_ascii_lowercase();
            let have = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            want == have
        }
    }
}

fn walk_into(dir: &Path, depth: usize, opts: &WalkOptions, out: &mut Vec<PathBuf>) -> Result<()> {
    if let Some(max) = opts.max_depth {
        if depth > max {
            return Ok(());
        }
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // An unreadable directory is skipped, not fatal: a permission error
        // three levels down should not abandon the whole scan.
        Err(_) => return Ok(()),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !opts.include_hidden && name.starts_with('.') {
            continue;
        }

        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        // Symlinks are skipped deliberately: following them turns a directory
        // scan into a potential infinite loop.
        if meta.file_type().is_symlink() {
            continue;
        }

        if meta.is_dir() {
            walk_into(&path, depth + 1, opts, out)?;
        } else if meta.is_file() && size_allowed(meta.len(), opts) && ext_allowed(&path, opts) {
            out.push(path);
        }
    }

    Ok(())
}

/// Collect files under `root`. A file passed in is returned as itself.
pub fn walk_files(root: &Path, opts: &WalkOptions) -> Result<Vec<PathBuf>> {
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if !root.exists() {
        return Err(Error::NotFound(root.display().to_string()));
    }
    let mut out = Vec::new();
    walk_into(root, 0, opts, &mut out)?;
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Path safety
// ---------------------------------------------------------------------------

/// Refuse a path that does not exist, with a message naming it.
pub fn require_exists(path: &Path) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(Error::NotFound(path.display().to_string()))
    }
}

/// True when `candidate` sits inside `root` (or is `root`).
///
/// Used before anything destructive touches a path supplied by the caller.
pub fn is_within(root: &Path, candidate: &Path) -> bool {
    let root = match std::fs::canonicalize(root) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let candidate = match std::fs::canonicalize(candidate) {
        Ok(p) => p,
        Err(_) => return false,
    };
    candidate.starts_with(&root)
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
        p.push(format!("savehorse-core-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create scratch dir");
        p
    }

    #[test]
    fn formats_bytes_across_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024 * 3), "3.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn parses_sizes_with_suffixes() {
        assert_eq!(parse_size("10").unwrap(), 10);
        assert_eq!(parse_size("10b").unwrap(), 10);
        assert_eq!(parse_size("10k").unwrap(), 10240);
        assert_eq!(parse_size("10K").unwrap(), 10240);
        assert_eq!(parse_size("2m").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size("1g").unwrap(), 1024 * 1024 * 1024);
        assert!(parse_size("banana").is_err());
    }

    #[test]
    fn crc32_matches_known_value() {
        // The IEEE CRC-32 of "123456789" is 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn sha256_matches_known_value() {
        let dir = scratch("sha");
        let file = dir.join("a.txt");
        fs::write(&file, b"abc").unwrap();
        let digest = sha256_file(&file).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hex_is_lowercase_and_padded() {
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    }

    #[test]
    fn walk_respects_depth_and_extension() {
        let dir = scratch("walk");
        fs::write(dir.join("top.txt"), b"a").unwrap();
        fs::write(dir.join("top.bin"), b"a").unwrap();
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("deep.txt"), b"a").unwrap();
        fs::write(dir.join(".hidden.txt"), b"a").unwrap();

        let all = walk_files(&dir, &WalkOptions::default()).unwrap();
        assert_eq!(all.len(), 3, "hidden file is skipped, two nested files found");

        let shallow = walk_files(
            &dir,
            &WalkOptions {
                max_depth: Some(0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(shallow.len(), 2, "depth 0 sees only the root's own files");

        let only_txt = walk_files(
            &dir,
            &WalkOptions {
                ext: Some("txt".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(only_txt.len(), 2, "top.txt and nested/deep.txt");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_returns_a_single_file_unchanged() {
        let dir = scratch("single");
        let file = dir.join("only.txt");
        fs::write(&file, b"x").unwrap();
        let found = walk_files(&file, &WalkOptions::default()).unwrap();
        assert_eq!(found, vec![file.clone()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn walk_errors_on_missing_root() {
        let missing = std::env::temp_dir().join("savehorse-does-not-exist-xyz");
        assert!(walk_files(&missing, &WalkOptions::default()).is_err());
    }

    #[test]
    fn is_within_accepts_child_and_rejects_sibling() {
        let dir = scratch("within");
        let child = dir.join("child");
        fs::create_dir_all(&child).unwrap();
        assert!(is_within(&dir, &child));
        assert!(is_within(&dir, &dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
