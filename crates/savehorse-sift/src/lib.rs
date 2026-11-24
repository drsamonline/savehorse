//! sift — find, extract and replace text.
//!
//! Ported from `sift` 1.0.0. The behaviour is unchanged except in two places,
//! both deliberate:
//!
//! 1. **No zip support.** The original searched inside `.zip` archives. That
//!    needs the `zip` crate; it is deferred rather than dropped, and the
//!    manifest records the operation as partial until it returns.
//! 2. **No printing.** The original wrote its results to stdout from inside
//!    the core. Here every function returns data and the caller decides how to
//!    show it — which is what lets the same code serve the CLI and the desktop
//!    app.

pub mod text;

pub use text::*;

use regex::{Regex, RegexBuilder};
use serde::Serialize;
use savehorse_core::{Error, Result, WalkOptions};
use std::fs;
use std::path::{Path, PathBuf};

/// One printable run found inside a binary.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct StringEntry {
    pub offset_hex: String,
    pub offset_dec: usize,
    pub encoding: String,
    pub content: String,
}

/// One matching line.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub file: String,
    pub line_number: usize,
    pub content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// One place where an XOR-obfuscated string decoded to something readable.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct XorHit {
    pub key: u8,
    pub offset: usize,
    pub context: String,
}

/// What a replacement run did.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ReplaceOutcome {
    pub replacements: usize,
    pub files_modified: usize,
    pub dry_run: bool,
    pub log: Vec<String>,
}

// ---------------------------------------------------------------------------
// strings
// ---------------------------------------------------------------------------

/// Extract printable ASCII and UTF-16LE runs of at least `min_len` characters.
pub fn extract_strings(
    path: &Path,
    min_len: usize,
    ascii_only: bool,
    unicode_only: bool,
) -> Result<Vec<StringEntry>> {
    let data = fs::read(path)?;
    let mut entries = Vec::new();

    if !unicode_only {
        let mut current: Vec<u8> = Vec::new();
        let mut start = 0usize;

        for (idx, &b) in data.iter().enumerate() {
            if (0x20..=0x7E).contains(&b) || b == b'\t' {
                if current.is_empty() {
                    start = idx;
                }
                current.push(b);
            } else {
                if current.len() >= min_len {
                    if let Ok(s) = String::from_utf8(current.clone()) {
                        entries.push(StringEntry {
                            offset_hex: format!("0x{:08x}", start),
                            offset_dec: start,
                            encoding: "ASCII".to_string(),
                            content: s,
                        });
                    }
                }
                current.clear();
            }
        }
        if current.len() >= min_len {
            if let Ok(s) = String::from_utf8(current) {
                entries.push(StringEntry {
                    offset_hex: format!("0x{:08x}", start),
                    offset_dec: start,
                    encoding: "ASCII".to_string(),
                    content: s,
                });
            }
        }
    }

    if !ascii_only && data.len() >= 2 {
        let mut current: Vec<char> = Vec::new();
        let mut start = 0usize;
        let mut i = 0usize;

        while i + 1 < data.len() {
            let b0 = data[i];
            let b1 = data[i + 1];

            if b1 == 0x00 && ((0x20..=0x7E).contains(&b0) || b0 == b'\t') {
                if current.is_empty() {
                    start = i;
                }
                current.push(b0 as char);
                i += 2;
            } else {
                if current.len() >= min_len {
                    let s: String = current.iter().collect();
                    entries.push(StringEntry {
                        offset_hex: format!("0x{:08x}", start),
                        offset_dec: start,
                        encoding: "UTF-16LE".to_string(),
                        content: s,
                    });
                }
                current = Vec::new();
                i += 1;
            }
        }
        if current.len() >= min_len {
            let s: String = current.iter().collect();
            entries.push(StringEntry {
                offset_hex: format!("0x{:08x}", start),
                offset_dec: start,
                encoding: "UTF-16LE".to_string(),
                content: s,
            });
        }
    }

    entries.sort_by_key(|e| e.offset_dec);
    Ok(entries)
}

// ---------------------------------------------------------------------------
// search
// ---------------------------------------------------------------------------

/// Turn a user glob such as `*.rs` into an anchored regex.
fn glob_regex(glob: &str) -> Result<Regex> {
    let pattern = format!(
        "^{}$",
        regex::escape(glob).replace(r"\*", ".*").replace(r"\?", ".")
    );
    Regex::new(&pattern).map_err(|e| Error::Invalid(format!("invalid glob: {}", e)))
}

fn build_matcher(pattern: &str, is_regex: bool, ignore_case: bool) -> Result<Regex> {
    let source = if is_regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };
    RegexBuilder::new(&source)
        .case_insensitive(ignore_case)
        .build()
        .map_err(|e| Error::Invalid(format!("invalid pattern: {}", e)))
}

fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in paths {
        let found = savehorse_core::walk_files(
            root,
            &WalkOptions {
                include_hidden: true,
                ..Default::default()
            },
        )?;
        files.extend(found);
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Search files for a pattern. A file that is not valid UTF-8 is skipped.
pub fn search_paths(
    pattern: &str,
    paths: &[PathBuf],
    is_regex: bool,
    ignore_case: bool,
    glob: Option<&str>,
) -> Result<Vec<SearchResult>> {
    let re = build_matcher(pattern, is_regex, ignore_case)?;
    let glob_re = match glob {
        Some(g) => Some(glob_regex(g)?),
        None => None,
    };

    let mut results = Vec::new();
    for path in expand(paths)? {
        if let Some(ref gre) = glob_re {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !gre.is_match(name) {
                continue;
            }
        }

        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            // Binary or unreadable: not an error, just not a text match.
            Err(_) => continue,
        };

        for (idx, line) in text.lines().enumerate() {
            if let Some(m) = re.find(line) {
                results.push(SearchResult {
                    file: savehorse_core::display_path(&path),
                    line_number: idx + 1,
                    content: line.to_string(),
                    match_start: m.start(),
                    match_end: m.end(),
                });
            }
        }
    }
    Ok(results)
}

// ---------------------------------------------------------------------------
// replace
// ---------------------------------------------------------------------------

/// Replace every match across the given paths.
///
/// With `dry_run` nothing is written and the log describes what would happen.
/// With `backup` a `.bak` copy is made beside each file before it changes.
pub fn replace_paths(
    pattern: &str,
    replacement: &str,
    paths: &[PathBuf],
    ignore_case: bool,
    dry_run: bool,
    backup: bool,
    glob: Option<&str>,
) -> Result<ReplaceOutcome> {
    let re = build_matcher(pattern, true, ignore_case)?;
    let glob_re = match glob {
        Some(g) => Some(glob_regex(g)?),
        None => None,
    };

    let mut outcome = ReplaceOutcome {
        replacements: 0,
        files_modified: 0,
        dry_run,
        log: Vec::new(),
    };

    for path in expand(paths)? {
        if let Some(ref gre) = glob_re {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !gre.is_match(name) {
                continue;
            }
        }

        let text = match fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let hits = re.find_iter(&text).count();
        if hits == 0 {
            continue;
        }

        outcome.replacements += hits;
        outcome.files_modified += 1;

        if dry_run {
            outcome.log.push(format!(
                "[preview] {}: would replace {} occurrence(s)",
                savehorse_core::display_path(&path),
                hits
            ));
            continue;
        }

        if backup {
            let bak = path.with_extension(format!(
                "{}.bak",
                path.extension().and_then(|e| e.to_str()).unwrap_or("")
            ));
            fs::copy(&path, &bak)?;
        }

        let replaced = re.replace_all(&text, replacement).to_string();
        fs::write(&path, replaced)?;
        outcome
            .log
            .push(format!("{}: replaced {} occurrence(s)", savehorse_core::display_path(&path), hits));
    }

    Ok(outcome)
}

// ---------------------------------------------------------------------------
// xor
// ---------------------------------------------------------------------------

/// Try every single-byte XOR key against `pattern` and report where it hits.
///
/// Useful when a string in a binary has been hidden by a one-byte XOR. With
/// `max_key` below 255 only the low keys are tried, which is faster on large
/// files.
pub fn xor_search(pattern: &str, file: &Path, max_key: u8) -> Result<Vec<XorHit>> {
    let data = fs::read(file)?;
    let target = pattern.as_bytes();

    if target.is_empty() {
        return Err(Error::Invalid("pattern cannot be empty".to_string()));
    }
    if target.len() > data.len() {
        return Ok(Vec::new());
    }

    let mut hits = Vec::new();
    for key in 0u8..=max_key {
        let encoded: Vec<u8> = target.iter().map(|b| b ^ key).collect();
        let last = data.len() - encoded.len();

        for i in 0..=last {
            if &data[i..i + encoded.len()] == &encoded[..] {
                hits.push(XorHit {
                    key,
                    offset: i,
                    context: String::from_utf8_lossy(&data[i..i + encoded.len()]).to_string(),
                });
            }
        }
    }

    hits.sort_by_key(|h| h.offset);
    Ok(hits)
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
        p.push(format!("savehorse-sift-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn extracts_ascii_strings_and_skips_short_runs() {
        let dir = scratch("strings");
        let file = dir.join("blob.bin");
        // "hello" then a NUL, then "hi" (too short), then "world!"
        fs::write(&file, b"\x00\x01hello\x00\xffhi\x00world!\x00").unwrap();

        let found = extract_strings(&file, 4, true, false).unwrap();
        let texts: Vec<&str> = found.iter().map(|e| e.content.as_str()).collect();
        assert_eq!(texts, vec!["hello", "world!"]);
        assert_eq!(found[0].encoding, "ASCII");
        assert!(found[0].offset_hex.starts_with("0x"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extracts_utf16_strings() {
        let dir = scratch("utf16");
        let file = dir.join("wide.bin");
        let mut bytes = Vec::new();
        for c in "Hello".bytes() {
            bytes.push(c);
            bytes.push(0x00);
        }
        fs::write(&file, &bytes).unwrap();

        let found = extract_strings(&file, 4, false, true).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].content, "Hello");
        assert_eq!(found[0].encoding, "UTF-16LE");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_finds_lines_by_line_number() {
        let dir = scratch("search");
        let file = dir.join("a.txt");
        fs::write(&file, "alpha\nbeta\nalpha again\n").unwrap();

        let hits = search_paths("alpha", &[file.clone()], false, false, None).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line_number, 1);
        assert_eq!(hits[1].line_number, 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_honours_case_and_regex() {
        let dir = scratch("search-case");
        let file = dir.join("a.txt");
        fs::write(&file, "Alpha\nalpha\n").unwrap();

        let sensitive = search_paths("alpha", &[file.clone()], false, false, None).unwrap();
        assert_eq!(sensitive.len(), 1);

        let insensitive = search_paths("alpha", &[file.clone()], false, true, None).unwrap();
        assert_eq!(insensitive.len(), 2);

        let regex = search_paths("^Alph", &[file.clone()], true, false, None).unwrap();
        assert_eq!(regex.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_respects_a_glob_filter() {
        let dir = scratch("glob");
        fs::write(dir.join("a.rs"), "needle").unwrap();
        fs::write(dir.join("b.txt"), "needle").unwrap();

        let hits = search_paths("needle", &[dir.clone()], false, false, Some("*.rs")).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].file.ends_with("a.rs"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_preview_writes_nothing() {
        let dir = scratch("replace-dry");
        let file = dir.join("a.txt");
        fs::write(&file, "cat cat").unwrap();

        let outcome = replace_paths("cat", "dog", &[file.clone()], false, true, false, None).unwrap();
        assert_eq!(outcome.replacements, 2);
        assert_eq!(outcome.files_modified, 1);
        assert!(outcome.dry_run);
        assert_eq!(fs::read_to_string(&file).unwrap(), "cat cat");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_writes_and_keeps_a_backup() {
        let dir = scratch("replace-real");
        let file = dir.join("a.txt");
        fs::write(&file, "cat cat").unwrap();

        let outcome = replace_paths("cat", "dog", &[file.clone()], false, false, true, None).unwrap();
        assert_eq!(outcome.replacements, 2);
        assert_eq!(fs::read_to_string(&file).unwrap(), "dog dog");
        assert!(dir.join("a.txt.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn xor_recovers_a_single_byte_key() {
        let dir = scratch("xor");
        let file = dir.join("obf.bin");
        let key = 0x5Au8;
        let encoded: Vec<u8> = b"password".iter().map(|b| b ^ key).collect();
        fs::write(&file, &encoded).unwrap();

        let hits = xor_search("password", &file, 255).unwrap();
        assert!(hits.iter().any(|h| h.key == key && h.offset == 0));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn xor_rejects_an_empty_pattern() {
        let dir = scratch("xor-empty");
        let file = dir.join("x.bin");
        fs::write(&file, b"data").unwrap();
        assert!(xor_search("", &file, 255).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
