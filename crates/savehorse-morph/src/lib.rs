//! morph — the rename engine.
//!
//! Ported from `morph` 1.0.0 (generation 2), including its undo journal.
//!
//! The original wrote the journal *after* the renames and ignored the result
//! (`let _ = fs::write(...)`). Two things follow from that: a crash halfway
//! through left files renamed with no record of how to put them back, and a
//! journal that failed to write was not noticed at all. Here the journal is
//! written **before** anything is renamed, and a failure to write it stops the
//! whole operation — a rename you cannot undo is worse than one you did not do.
//!
//! One level of directory is read, not a whole tree. That is the original's
//! behaviour and it is deliberate for a rename tool: a recursive rename is one
//! bad pattern away from touching a whole disk.

use regex::Regex;
use savehorse_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Which case transformation to apply to the stem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseMode {
    Lower,
    Upper,
    Title,
    Snake,
}

impl CaseMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lower" => Ok(CaseMode::Lower),
            "upper" => Ok(CaseMode::Upper),
            "title" => Ok(CaseMode::Title),
            "snake" => Ok(CaseMode::Snake),
            other => Err(Error::Invalid(format!(
                "unknown case '{}': use lower, upper, title or snake",
                other
            ))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RenameItem {
    pub original_path: String,
    pub original_name: String,
    pub new_name: String,
    pub new_path: String,
    pub changed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Journal {
    pub timestamp: String,
    pub operations: Vec<RenameItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RenameReport {
    pub dry_run: bool,
    pub considered: usize,
    pub changed: usize,
    pub journal_path: Option<String>,
    pub items: Vec<RenameItem>,
}

/// Everything a rename run can be told to do.
#[derive(Debug, Clone, Default)]
pub struct RenameOptions {
    /// A regular expression applied to the stem.
    pub pattern: Option<String>,
    pub replace: String,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
    pub case: Option<CaseMode>,
    pub extension: Option<String>,
    pub lowercase_extension: bool,
    pub number: bool,
    pub start: usize,
    pub step: usize,
    pub pad: usize,
}

impl RenameOptions {
    fn default_numbering() -> Self {
        RenameOptions {
            start: 1,
            step: 1,
            pad: 3,
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Case helpers
// ---------------------------------------------------------------------------

pub fn to_title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut capitalise = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if capitalise {
                out.extend(c.to_uppercase());
                capitalise = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
            capitalise = true;
        }
    }
    out
}

pub fn to_snake_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut previous_was_separator = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            out.push('_');
            previous_was_separator = true;
        }
    }
    out.trim_matches('_').to_string()
}

/// Files named directly by the paths given, plus the files one level inside any
/// directory among them.
pub fn collect_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            files.push(path.clone());
        } else if path.is_dir() {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let candidate = entry.path();
                    if candidate.is_file() {
                        files.push(candidate);
                    }
                }
            }
        }
    }
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// Work out every new name, and refuse the run if any of them collides.
fn plan(paths: &[PathBuf], options: &RenameOptions) -> Result<Vec<RenameItem>> {
    let targets = collect_files(paths);
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let regex = match options.pattern {
        Some(ref p) if !p.is_empty() => Some(
            Regex::new(p).map_err(|e| Error::Invalid(format!("invalid pattern: {}", e)))?,
        ),
        _ => None,
    };

    let mut planned = Vec::with_capacity(targets.len());
    let mut counter = options.start;

    for file in &targets {
        let parent = file.parent().unwrap_or_else(|| Path::new(""));
        let original_name = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let current_extension = file.extension().map(|e| e.to_string_lossy().to_string());

        let mut new_stem = stem.clone();

        if let Some(ref re) = regex {
            new_stem = re.replace_all(&new_stem, options.replace.as_str()).to_string();
        }

        if let Some(mode) = options.case {
            new_stem = match mode {
                CaseMode::Lower => new_stem.to_lowercase(),
                CaseMode::Upper => new_stem.to_uppercase(),
                CaseMode::Title => to_title_case(&new_stem),
                CaseMode::Snake => to_snake_case(&new_stem),
            };
        }

        if let Some(ref prefix) = options.prefix {
            if !prefix.is_empty() {
                new_stem = format!("{}{}", prefix, new_stem);
            }
        }
        if let Some(ref suffix) = options.suffix {
            if !suffix.is_empty() {
                new_stem = format!("{}{}", new_stem, suffix);
            }
        }

        if options.number {
            new_stem = format!(
                "{}_{}",
                new_stem,
                format!("{:0width$}", counter, width = options.pad.max(1))
            );
            counter += options.step.max(1);
        }

        let new_extension = match options.extension {
            Some(ref e) => {
                let clean = e.trim_start_matches('.');
                if clean.is_empty() {
                    None
                } else if options.lowercase_extension {
                    Some(clean.to_lowercase())
                } else {
                    Some(clean.to_string())
                }
            }
            None => current_extension.map(|e| {
                if options.lowercase_extension {
                    e.to_lowercase()
                } else {
                    e
                }
            }),
        };

        let new_name = match new_extension {
            Some(e) => format!("{}.{}", new_stem, e),
            None => new_stem,
        };

        let changed = original_name != new_name;
        let new_path = parent.join(&new_name).to_string_lossy().to_string();

        planned.push(RenameItem {
            original_path: file.to_string_lossy().to_string(),
            original_name,
            new_name,
            new_path,
            changed,
        });
    }

    // Two files may not end up with the same name, and a name may not land on
    // something that is already there and not part of this run.
    let mut destinations: HashSet<String> = HashSet::new();
    for item in planned.iter().filter(|i| i.changed) {
        if !destinations.insert(item.new_path.to_lowercase()) {
            return Err(Error::Invalid(format!(
                "two files would become '{}'",
                item.new_path
            )));
        }

        let is_self = item.original_path.eq_ignore_ascii_case(&item.new_path);
        let is_in_run = targets
            .iter()
            .any(|t| t.to_string_lossy().eq_ignore_ascii_case(&item.new_path));

        if Path::new(&item.new_path).exists() && !is_self && !is_in_run {
            return Err(Error::Invalid(format!(
                "'{}' already exists on disk",
                item.new_path
            )));
        }
    }

    Ok(planned)
}

/// Rename the files, or describe what would happen.
///
/// With `dry_run` nothing is written and no journal is produced.
pub fn rename(
    paths: &[PathBuf],
    options: &RenameOptions,
    dry_run: bool,
    journal_path: Option<&Path>,
) -> Result<RenameReport> {
    let planned = plan(paths, options)?;
    let changed: Vec<RenameItem> = planned.iter().filter(|i| i.changed).cloned().collect();

    if dry_run || changed.is_empty() {
        return Ok(RenameReport {
            dry_run,
            considered: planned.len(),
            changed: changed.len(),
            journal_path: None,
            items: planned,
        });
    }

    // Write the record first. A rename that cannot be undone is worse than a
    // rename that did not happen.
    let journal_file = journal_path.map(|p| p.to_path_buf());
    if let Some(ref path) = journal_file {
        let journal = Journal {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_string()),
            operations: changed.clone(),
        };
        let text = serde_json::to_string_pretty(&journal)
            .map_err(|e| Error::Invalid(format!("could not build the journal: {}", e)))?;
        fs::write(path, text).map_err(|e| {
            Error::Invalid(format!(
                "could not write the undo journal to '{}': {} — nothing was renamed",
                path.display(),
                e
            ))
        })?;
    }

    for (index, item) in changed.iter().enumerate() {
        let case_only = item.original_path.eq_ignore_ascii_case(&item.new_path);
        if case_only {
            // On a case-insensitive filesystem a direct rename is a no-op, so
            // step through a temporary name.
            let temp = format!("{}.savehorse_tmp_{}", item.original_path, index);
            fs::rename(&item.original_path, &temp)?;
            fs::rename(&temp, &item.new_path)?;
        } else {
            fs::rename(&item.original_path, &item.new_path)?;
        }
    }

    Ok(RenameReport {
        dry_run: false,
        considered: planned.len(),
        changed: changed.len(),
        journal_path: journal_file.map(|p| p.to_string_lossy().to_string()),
        items: planned,
    })
}

/// Put every rename in a journal back, newest first.
pub fn undo(journal_path: &Path, dry_run: bool) -> Result<RenameReport> {
    if !journal_path.exists() {
        return Err(Error::NotFound(journal_path.display().to_string()));
    }

    let text = fs::read_to_string(journal_path)?;
    let journal: Journal = serde_json::from_str(&text)
        .map_err(|e| Error::Invalid(format!("not a rename journal: {}", e)))?;

    let mut items = Vec::with_capacity(journal.operations.len());

    for item in journal.operations.iter().rev() {
        if !Path::new(&item.new_path).exists() && !dry_run {
            return Err(Error::Invalid(format!(
                "cannot undo: '{}' is no longer there",
                item.new_path
            )));
        }

        items.push(RenameItem {
            original_path: item.new_path.clone(),
            original_name: item.new_name.clone(),
            new_name: item.original_name.clone(),
            new_path: item.original_path.clone(),
            changed: true,
        });
    }

    if !dry_run {
        for (index, item) in items.iter().enumerate() {
            let case_only = item.original_path.eq_ignore_ascii_case(&item.new_path);
            if case_only {
                let temp = format!("{}.savehorse_undo_{}", item.original_path, index);
                fs::rename(&item.original_path, &temp)?;
                fs::rename(&temp, &item.new_path)?;
            } else {
                fs::rename(&item.original_path, &item.new_path)?;
            }
        }
        // The journal has served its purpose.
        fs::remove_file(journal_path)?;
    }

    Ok(RenameReport {
        dry_run,
        considered: items.len(),
        changed: items.len(),
        journal_path: None,
        items,
    })
}

/// Options with the numbering defaults filled in.
pub fn options_with_defaults() -> RenameOptions {
    RenameOptions::default_numbering()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("savehorse-morph-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn title_and_snake_case() {
        assert_eq!(to_title_case("hello world"), "Hello World");
        assert_eq!(to_title_case("hELLo wORLD"), "Hello World");
        assert_eq!(to_snake_case("Hello World"), "hello_world");
        assert_eq!(to_snake_case("a--b  c"), "a_b_c");
        assert_eq!(to_snake_case("__x__"), "x");
    }

    #[test]
    fn case_parsing_accepts_known_modes_only() {
        assert_eq!(CaseMode::parse("SNAKE").unwrap(), CaseMode::Snake);
        assert!(CaseMode::parse("camel").is_err());
    }

    #[test]
    fn dry_run_renames_nothing() {
        let dir = scratch("dry");
        fs::write(dir.join("hello.txt"), b"x").unwrap();

        let options = RenameOptions {
            case: Some(CaseMode::Upper),
            ..Default::default()
        };
        let report = rename(&[dir.clone()], &options, true, None).unwrap();

        assert!(report.dry_run);
        assert_eq!(report.changed, 1);
        assert_eq!(report.items[0].new_name, "HELLO.txt");
        assert!(dir.join("hello.txt").exists(), "preview must not rename");
        assert!(!dir.join("HELLO.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_applies_and_writes_a_journal() {
        let dir = scratch("apply");
        fs::write(dir.join("one.txt"), b"1").unwrap();
        fs::write(dir.join("two.txt"), b"2").unwrap();

        let journal = dir.join("journal.json");
        let options = RenameOptions {
            prefix: Some("img_".to_string()),
            number: true,
            ..options_with_defaults()
        };
        let report = rename(&[dir.clone()], &options, false, Some(&journal)).unwrap();

        assert_eq!(report.changed, 2);
        assert!(journal.exists());
        assert!(dir.join("img_one_001.txt").exists());
        assert!(dir.join("img_two_002.txt").exists());
        assert!(!dir.join("one.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_puts_everything_back() {
        let dir = scratch("undo");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("b.txt"), b"b").unwrap();

        let journal = dir.join("journal.json");
        let options = RenameOptions {
            suffix: Some("_x".to_string()),
            ..Default::default()
        };
        rename(&[dir.clone()], &options, false, Some(&journal)).unwrap();
        assert!(dir.join("a_x.txt").exists());

        let report = undo(&journal, false).unwrap();
        assert_eq!(report.changed, 2);
        assert!(dir.join("a.txt").exists());
        assert!(dir.join("b.txt").exists());
        assert!(!journal.exists(), "the journal is consumed by a successful undo");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn undo_preview_leaves_files_alone() {
        let dir = scratch("undo-dry");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        let journal = dir.join("journal.json");
        let options = RenameOptions {
            suffix: Some("_x".to_string()),
            ..Default::default()
        };
        rename(&[dir.clone()], &options, false, Some(&journal)).unwrap();

        undo(&journal, true).unwrap();
        assert!(dir.join("a_x.txt").exists(), "preview must not undo");
        assert!(journal.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_collision_is_refused_before_anything_moves() {
        let dir = scratch("collision");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("b.txt"), b"b").unwrap();

        // Both stems become the same.
        let options = RenameOptions {
            pattern: Some(".*".to_string()),
            replace: "same".to_string(),
            ..Default::default()
        };
        let err = rename(&[dir.clone()], &options, false, None).unwrap_err();
        assert!(err.to_string().contains("would become"), "got: {}", err);
        assert!(dir.join("a.txt").exists() && dir.join("b.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_unrelated_file_blocks_the_rename() {
        let dir = scratch("existing");
        fs::write(dir.join("a.txt"), b"a").unwrap();
        fs::write(dir.join("b.txt"), b"b").unwrap();

        // a.txt -> b.txt, which already exists and is not part of the run.
        let options = RenameOptions {
            pattern: Some("^a$".to_string()),
            replace: "b".to_string(),
            ..Default::default()
        };
        let err = rename(&[dir.join("a.txt")], &options, false, None).unwrap_err();
        assert!(err.to_string().contains("already exists"), "got: {}", err);
        assert_eq!(fs::read(dir.join("b.txt")).unwrap(), b"b", "b must be untouched");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_can_be_replaced_and_lowercased() {
        let dir = scratch("ext");
        fs::write(dir.join("photo.PNG"), b"x").unwrap();

        let options = RenameOptions {
            extension: Some(".jpg".to_string()),
            lowercase_extension: true,
            ..Default::default()
        };
        let report = rename(&[dir.clone()], &options, false, None).unwrap();
        assert_eq!(report.items[0].new_name, "photo.jpg");
        assert!(dir.join("photo.jpg").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
