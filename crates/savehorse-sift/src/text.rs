//! Text operations from `slate` 1.0.0 (generation 2).
//!
//! These live in the sift crate because they are the same job: read text,
//! change it, report on it. Keeping them apart would mean two crates that
//! always ship together.
//!
//! Two deliberate differences from the original:
//!
//! * `convert` will not write in place. The original overwrote the source when
//!   no output was given, which is a surprising thing for a conversion to do;
//!   an explicit `--out` is now required.
//! * `view` ends lines with `\n` rather than `\r\n`, which was leaving stray
//!   carriage returns in every terminal on Unix.

use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ViewResult {
    pub file: String,
    pub encoding: String,
    pub total_lines: usize,
    pub lines_shown: usize,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EolResult {
    pub file: String,
    pub crlf_count: usize,
    pub lf_count: usize,
    pub cr_count: usize,
    pub detected_dominant: String,
    pub converted_to: Option<String>,
    pub in_place: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CleanResult {
    pub file: String,
    pub lines_processed: usize,
    pub trailing_spaces_removed: usize,
    pub tabs_expanded: usize,
    pub blank_lines_collapsed: usize,
    pub in_place: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DocumentStats {
    pub file: String,
    pub total_bytes: u64,
    pub total_characters: usize,
    pub total_words: usize,
    pub unique_words: usize,
    pub total_lines: usize,
    pub empty_lines: usize,
    pub average_word_length: f64,
    pub longest_line_chars: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EncodingResult {
    pub file: String,
    pub detected_encoding: String,
    pub has_bom: bool,
    pub byte_count: u64,
    pub converted_to: Option<String>,
    pub output_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Reading and writing
// ---------------------------------------------------------------------------

/// Read a file as text, decoding a byte-order mark when one is present.
pub fn read_text(path: &Path) -> Result<(String, String, bool)> {
    let mut file = File::open(path)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;

    if raw.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok((
            String::from_utf8_lossy(&raw[3..]).to_string(),
            "UTF-8 with BOM".to_string(),
            true,
        ));
    }

    if raw.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Ok((
            String::from_utf16_lossy(&units),
            "UTF-16LE with BOM".to_string(),
            true,
        ));
    }

    match std::str::from_utf8(&raw) {
        Ok(s) => Ok((s.to_string(), "UTF-8".to_string(), false)),
        Err(_) => Ok((
            raw.iter().map(|&b| b as char).collect(),
            "Windows-1252 / ISO-8859-1".to_string(),
            false,
        )),
    }
}

/// Write through a temporary file in the same directory, then rename.
///
/// A crash mid-write leaves the original intact rather than half-rewritten.
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp = parent.join(format!(".savehorse_tmp_{}_{}", std::process::id(), unique));

    {
        let mut file = File::create(&temp)?;
        file.write_all(content)?;
        file.flush()?;
    }

    fs::rename(&temp, path).map_err(|e| {
        let _ = fs::remove_file(&temp);
        Error::Io(e)
    })
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Print a file, optionally numbered, from a starting line for a count.
pub fn view(path: &Path, numbers: bool, start: usize, count: Option<usize>) -> Result<ViewResult> {
    let (content, encoding, _bom) = read_text(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let first = start.saturating_sub(1).min(total_lines);
    let rest = &lines[first..];
    let take = match count {
        Some(c) => c.min(rest.len()),
        None => rest.len(),
    };

    let mut text = String::new();
    for (i, line) in rest[..take].iter().enumerate() {
        if numbers {
            text.push_str(&format!("{:>6} | {}\n", first + i + 1, line));
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }

    Ok(ViewResult {
        file: path.to_string_lossy().to_string(),
        encoding,
        total_lines,
        lines_shown: take,
        text,
    })
}

/// Count line endings, and optionally rewrite them to one style.
pub fn normalize_eol(path: &Path, to: Option<&str>, in_place: bool) -> Result<EolResult> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;

    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\r' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                crlf += 1;
                i += 1;
            } else {
                cr += 1;
            }
        } else if bytes[i] == b'\n' {
            lf += 1;
        }
        i += 1;
    }

    let dominant = if crlf >= lf && crlf >= cr {
        "CRLF (Windows)"
    } else if lf >= cr {
        "LF (Unix)"
    } else {
        "CR (classic Mac)"
    };

    let mut converted_to = None;

    if let Some(target) = to {
        let sequence = match target.to_lowercase().as_str() {
            "crlf" => "\r\n",
            "lf" => "\n",
            "cr" => "\r",
            other => {
                return Err(Error::Invalid(format!(
                    "unknown line ending '{}': use crlf, lf or cr",
                    other
                )))
            }
        };

        let text = String::from_utf8_lossy(&bytes).to_string();
        let joined = format!("{}{}", text.lines().collect::<Vec<_>>().join(sequence), sequence);

        if in_place {
            atomic_write(path, joined.as_bytes())?;
        }
        converted_to = Some(target.to_uppercase());
    }

    Ok(EolResult {
        file: path.to_string_lossy().to_string(),
        crlf_count: crlf,
        lf_count: lf,
        cr_count: cr,
        detected_dominant: dominant.to_string(),
        converted_to,
        in_place,
    })
}

/// Strip trailing whitespace, expand tabs, collapse runs of blank lines.
///
/// Nothing is written unless `in_place` is set; otherwise the counts describe
/// what a real run would do.
pub fn clean_text(
    path: &Path,
    strip_trailing: bool,
    tab_size: Option<usize>,
    collapse_blank: bool,
    in_place: bool,
) -> Result<CleanResult> {
    let (content, _encoding, _bom) = read_text(path)?;

    let mut lines_processed = 0usize;
    let mut trailing_removed = 0usize;
    let mut tabs_expanded = 0usize;
    let mut blank_collapsed = 0usize;

    let tab_spaces = " ".repeat(tab_size.unwrap_or(4));
    let mut out: Vec<String> = Vec::new();
    let mut previous_blank = false;

    for line in content.lines() {
        lines_processed += 1;
        let mut current = line.to_string();

        if tab_size.is_some() && current.contains('\t') {
            tabs_expanded += current.matches('\t').count();
            current = current.replace('\t', &tab_spaces);
        }

        if strip_trailing {
            let trimmed = current.trim_end_matches([' ', '\t']);
            if trimmed.len() < current.len() {
                trailing_removed += current.len() - trimmed.len();
                current = trimmed.to_string();
            }
        }

        let is_blank = current.trim().is_empty();
        if collapse_blank && is_blank && previous_blank {
            blank_collapsed += 1;
            continue;
        }
        previous_blank = is_blank;

        out.push(current);
    }

    if in_place {
        let joined = format!("{}\n", out.join("\n"));
        atomic_write(path, joined.as_bytes())?;
    }

    Ok(CleanResult {
        file: path.to_string_lossy().to_string(),
        lines_processed,
        trailing_spaces_removed: trailing_removed,
        tabs_expanded,
        blank_lines_collapsed: blank_collapsed,
        in_place,
    })
}

/// Lines, words, vocabulary and shape of a document.
pub fn document_stats(path: &Path) -> Result<DocumentStats> {
    let (content, _encoding, _bom) = read_text(path)?;
    let total_bytes = fs::metadata(path)?.len();

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let mut empty_lines = 0usize;
    let mut longest_line = 0usize;
    let mut total_characters = 0usize;
    let mut total_words = 0usize;
    let mut word_characters = 0usize;
    let mut vocabulary: HashSet<String> = HashSet::new();

    for line in &lines {
        let chars = line.chars().count();
        total_characters += chars;
        if chars > longest_line {
            longest_line = chars;
        }
        if line.trim().is_empty() {
            empty_lines += 1;
        }

        for word in line.split_whitespace() {
            let clean = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if !clean.is_empty() {
                total_words += 1;
                word_characters += clean.chars().count();
                vocabulary.insert(clean);
            }
        }
    }

    let average = if total_words > 0 {
        ((word_characters as f64 / total_words as f64) * 100.0).round() / 100.0
    } else {
        0.0
    };

    Ok(DocumentStats {
        file: path.to_string_lossy().to_string(),
        total_bytes,
        total_characters,
        total_words,
        unique_words: vocabulary.len(),
        total_lines,
        empty_lines,
        average_word_length: average,
        longest_line_chars: longest_line,
    })
}

/// Re-encode a file to UTF-8, UTF-8 with BOM, UTF-16LE or ASCII.
///
/// An explicit destination is required: the original wrote back over the
/// source when none was given.
pub fn convert_encoding(path: &Path, to: &str, out: &Path) -> Result<EncodingResult> {
    let mut file = File::open(path)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let byte_count = raw.len() as u64;

    let (text, detected, has_bom) = read_text(path)?;

    let encoded: Vec<u8> = match to.to_lowercase().as_str() {
        "utf8" | "utf-8" => text.as_bytes().to_vec(),
        "utf8-bom" | "utf-8-bom" => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        "utf16le" | "utf-16le" => {
            let mut bytes = vec![0xFF, 0xFE];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes
        }
        "ascii" => text
            .chars()
            .map(|c| if (c as u32) < 128 { c as u8 } else { b'?' })
            .collect(),
        other => {
            return Err(Error::Invalid(format!(
                "unknown encoding '{}': use utf8, utf8-bom, utf16le or ascii",
                other
            )))
        }
    };

    atomic_write(out, &encoded)?;

    Ok(EncodingResult {
        file: path.to_string_lossy().to_string(),
        detected_encoding: detected,
        has_bom,
        byte_count,
        converted_to: Some(to.to_uppercase()),
        output_path: Some(out.to_string_lossy().to_string()),
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
        p.push(format!("savehorse-slate-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn view_numbers_lines_and_pages() {
        let dir = scratch("view");
        let file = dir.join("a.txt");
        fs::write(&file, "one\ntwo\nthree\nfour\n").unwrap();

        let all = view(&file, true, 1, None).unwrap();
        assert_eq!(all.total_lines, 4);
        assert_eq!(all.lines_shown, 4);
        assert!(all.text.contains("     1 | one"));
        assert!(all.text.contains("     4 | four"));

        let page = view(&file, false, 2, Some(2)).unwrap();
        assert_eq!(page.lines_shown, 2);
        assert_eq!(page.text, "two\nthree\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn eol_detects_and_converts() {
        let dir = scratch("eol");
        let file = dir.join("a.txt");
        fs::write(&file, b"a\r\nb\r\nc\r\n").unwrap();

        let report = normalize_eol(&file, None, false).unwrap();
        assert_eq!(report.crlf_count, 3);
        assert_eq!(report.lf_count, 0);
        assert!(report.detected_dominant.starts_with("CRLF"));
        assert_eq!(report.converted_to, None);

        // Convert to LF in place.
        let report = normalize_eol(&file, Some("lf"), true).unwrap();
        assert_eq!(report.converted_to.as_deref(), Some("LF"));
        assert_eq!(fs::read(&file).unwrap(), b"a\nb\nc\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn eol_rejects_an_unknown_style() {
        let dir = scratch("eol-bad");
        let file = dir.join("a.txt");
        fs::write(&file, b"x\n").unwrap();
        assert!(normalize_eol(&file, Some("crlfcrlf"), false).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_previews_before_it_writes() {
        let dir = scratch("clean");
        let file = dir.join("a.txt");
        fs::write(&file, "a   \n\n\n\nb\t\n").unwrap();
        let original = fs::read(&file).unwrap();

        let report = clean_text(&file, true, Some(4), true, false).unwrap();
        assert!(report.trailing_spaces_removed > 0);
        assert!(report.blank_lines_collapsed > 0);
        assert!(report.tabs_expanded > 0);
        assert!(!report.in_place);
        assert_eq!(fs::read(&file).unwrap(), original, "a preview must not write");

        let report = clean_text(&file, true, Some(4), true, true).unwrap();
        assert!(report.in_place);
        let cleaned = fs::read_to_string(&file).unwrap();
        assert!(!cleaned.contains("a   \n"), "trailing spaces should be gone");
        assert!(!cleaned.contains('\t'));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn stats_count_words_and_vocabulary() {
        let dir = scratch("stats");
        let file = dir.join("a.txt");
        fs::write(&file, "the cat and the dog\n\nsecond line\n").unwrap();

        let stats = document_stats(&file).unwrap();
        assert_eq!(stats.total_lines, 3);
        assert_eq!(stats.empty_lines, 1);
        assert_eq!(stats.total_words, 7);
        assert_eq!(stats.unique_words, 6, "the appears twice");
        assert!(stats.longest_line_chars > 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_writes_a_bom_when_asked() {
        let dir = scratch("convert");
        let file = dir.join("a.txt");
        fs::write(&file, "hello\n").unwrap();

        let out = dir.join("bom.txt");
        let report = convert_encoding(&file, "utf8-bom", &out).unwrap();
        assert_eq!(report.converted_to.as_deref(), Some("UTF8-BOM"));
        let bytes = fs::read(&out).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert_eq!(&bytes[3..], b"hello\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_to_ascii_replaces_wide_characters() {
        let dir = scratch("ascii");
        let file = dir.join("a.txt");
        fs::write(&file, "café\n").unwrap();

        let out = dir.join("ascii.txt");
        convert_encoding(&file, "ascii", &out).unwrap();
        assert_eq!(fs::read(&out).unwrap(), b"caf?\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn convert_rejects_an_unknown_encoding() {
        let dir = scratch("convert-bad");
        let file = dir.join("a.txt");
        fs::write(&file, "x").unwrap();
        assert!(convert_encoding(&file, "ebcdic", &dir.join("o.txt")).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
