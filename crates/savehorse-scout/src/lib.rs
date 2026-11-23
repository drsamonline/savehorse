//! scout — catalogue a filesystem and find duplicate files.
//!
//! Ported from `scout` 1.0.0. Two changes from the original:
//!
//! 1. **No printing and no clap.** The original core wrote reports to stdout
//!    and depended on `clap::ValueEnum` for its export formats. Here every
//!    function returns data or a `String`, and [`ExportFormat`] is a plain
//!    enum with its own parser.
//! 2. **Deletion returns a record.** `cmd_clean` used to delete and print.
//!    [`delete_duplicates`] deletes and returns exactly what it removed, so a
//!    caller can undo or audit.

use serde::{Deserialize, Serialize};
use savehorse_core::{format_bytes, Error, Result, WalkOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One file in a catalogue.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub path: String,
    pub relative_path: String,
    pub size_bytes: u64,
    pub size_human: String,
    pub extension: String,
}

/// One set of files that are byte-for-byte identical.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCluster {
    pub sha256: String,
    pub file_size_bytes: u64,
    pub file_size_human: String,
    pub count: usize,
    pub wasted_bytes: u64,
    pub original: String,
    pub duplicates: Vec<String>,
}

/// The result of a duplicate scan.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSummary {
    pub total_clusters: usize,
    pub total_duplicate_files: usize,
    pub total_wasted_bytes: u64,
    pub total_wasted_human: String,
    pub clusters: Vec<DuplicateCluster>,
}

/// What a cleanup actually removed.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct CleanOutcome {
    pub committed: bool,
    pub removed: Vec<String>,
    pub removed_count: usize,
    pub reclaimed_bytes: u64,
    pub reclaimed_human: String,
    pub kept: Vec<String>,
}

/// How a catalogue should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Table,
    Json,
    Csv,
    Markdown,
    Html,
}

impl ExportFormat {
    /// Parse a format name as typed on a command line.
    pub fn parse(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "table" | "text" => Ok(ExportFormat::Table),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            "markdown" | "md" => Ok(ExportFormat::Markdown),
            "html" => Ok(ExportFormat::Html),
            other => Err(Error::Invalid(format!(
                "unknown format '{}': use table, json, csv, markdown or html",
                other
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ExportFormat::Table => "table",
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Markdown => "markdown",
            ExportFormat::Html => "html",
        }
    }
}

// ---------------------------------------------------------------------------
// Catalogue
// ---------------------------------------------------------------------------

/// Walk `root` and describe every file in it.
pub fn build_catalog(root: &Path, opts: &WalkOptions) -> Result<Vec<CatalogEntry>> {
    let files = savehorse_core::walk_files(root, opts)?;
    let mut entries = Vec::with_capacity(files.len());

    for path in files {
        let size = match std::fs::metadata(&path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        entries.push(CatalogEntry {
            path: path.to_string_lossy().to_string(),
            relative_path: relative,
            size_bytes: size,
            size_human: format_bytes(size),
            extension: path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default(),
        });
    }

    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(entries)
}

/// Total size of a catalogue.
pub fn catalog_total_bytes(entries: &[CatalogEntry]) -> u64 {
    entries.iter().map(|e| e.size_bytes).sum()
}

fn csv_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a catalogue in one of the five formats.
pub fn render_catalog(entries: &[CatalogEntry], format: ExportFormat) -> Result<String> {
    let total = catalog_total_bytes(entries);

    let out = match format {
        ExportFormat::Table => {
            let mut s = String::new();
            s.push_str(&format!(
                "{:<60} {:<12} {}\n",
                "Relative Path", "Size", "Extension"
            ));
            s.push_str(&format!("{}\n", "-".repeat(82)));
            for e in entries {
                s.push_str(&format!(
                    "{:<60} {:<12} {}\n",
                    e.relative_path, e.size_human, e.extension
                ));
            }
            s.push_str(&format!("{}\n", "-".repeat(82)));
            s.push_str(&format!(
                "Total: {} files ({} bytes / {})\n",
                entries.len(),
                total,
                format_bytes(total)
            ));
            s
        }
        ExportFormat::Json => serde_json::to_string_pretty(entries)
            .map_err(|e| Error::Invalid(format!("could not encode JSON: {}", e)))?,
        ExportFormat::Csv => {
            let mut s = String::from("RelativePath,SizeBytes,SizeHuman,Extension,FullPath\n");
            for e in entries {
                s.push_str(&format!(
                    "{},{},{},{},{}\n",
                    csv_quote(&e.relative_path),
                    e.size_bytes,
                    csv_quote(&e.size_human),
                    csv_quote(&e.extension),
                    csv_quote(&e.path)
                ));
            }
            s
        }
        ExportFormat::Markdown => {
            let mut s = String::from(
                "| Relative Path | Size (Bytes) | Human Size | Extension |\n| :--- | :---: | :---: | :---: |\n",
            );
            for e in entries {
                s.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    e.relative_path, e.size_bytes, e.size_human, e.extension
                ));
            }
            s.push_str(&format!(
                "\n**Total:** {} files, {}\n",
                entries.len(),
                format_bytes(total)
            ));
            s
        }
        ExportFormat::Html => {
            let mut s = String::from(
                "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"UTF-8\">\n  <title>Filesystem Catalog</title>\n  <style>\n    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0f172a; color: #f8fafc; padding: 2rem; }\n    h1 { color: #38bdf8; }\n    table { width: 100%; border-collapse: collapse; margin-top: 1rem; }\n    th, td { text-align: left; padding: 8px 12px; border-bottom: 1px solid #334155; }\n    th { background: #1e293b; color: #94a3b8; }\n    tr:hover { background: #1e293b; }\n    .mono { font-family: monospace; }\n  </style>\n</head>\n<body>\n  <h1>Filesystem Catalog Report</h1>\n  <p>",
            );
            s.push_str(&format!(
                "Total Items: {} | Total Size: {}</p>\n",
                entries.len(),
                format_bytes(total)
            ));
            s.push_str("  <table>\n    <thead><tr><th>Relative Path</th><th>Size</th><th>Extension</th></tr></thead>\n    <tbody>\n");
            for e in entries {
                s.push_str(&format!(
                    "      <tr><td class=\"mono\">{}</td><td>{}</td><td>{}</td></tr>\n",
                    html_escape(&e.relative_path),
                    e.size_human,
                    html_escape(&e.extension)
                ));
            }
            s.push_str("    </tbody>\n  </table>\n</body>\n</html>");
            s
        }
    };

    Ok(out)
}

// ---------------------------------------------------------------------------
// Duplicates
// ---------------------------------------------------------------------------

/// Find files whose contents are identical.
///
/// Files are grouped by size first, so SHA-256 is only computed where a
/// collision is even possible. That is what keeps a scan of a large tree from
/// hashing every file.
pub fn find_duplicates(
    root: &Path,
    min_size: u64,
    ext: Option<&str>,
    max_depth: Option<usize>,
) -> Result<DuplicateSummary> {
    let opts = WalkOptions {
        max_depth,
        min_size: Some(min_size),
        max_size: None,
        ext: ext.map(|e| e.to_string()),
        include_hidden: false,
    };

    let entries = build_catalog(root, &opts)?;

    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for entry in &entries {
        by_size
            .entry(entry.size_bytes)
            .or_default()
            .push(PathBuf::from(&entry.path));
    }

    let mut clusters: Vec<DuplicateCluster> = Vec::new();

    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }

        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        for path in paths {
            if let Ok(hash) = savehorse_core::sha256_file(&path) {
                by_hash
                    .entry(hash)
                    .or_default()
                    .push(path.to_string_lossy().to_string());
            }
        }

        for (hash, mut files) in by_hash {
            if files.len() < 2 {
                continue;
            }
            files.sort();
            let count = files.len();
            clusters.push(DuplicateCluster {
                sha256: hash,
                file_size_bytes: size,
                file_size_human: format_bytes(size),
                count,
                wasted_bytes: (count as u64 - 1) * size,
                original: files[0].clone(),
                duplicates: files[1..].to_vec(),
            });
        }
    }

    clusters.sort_by(|a, b| b.wasted_bytes.cmp(&a.wasted_bytes));

    let total_wasted: u64 = clusters.iter().map(|c| c.wasted_bytes).sum();
    let total_dups: usize = clusters.iter().map(|c| c.count - 1).sum();

    Ok(DuplicateSummary {
        total_clusters: clusters.len(),
        total_duplicate_files: total_dups,
        total_wasted_bytes: total_wasted,
        total_wasted_human: format_bytes(total_wasted),
        clusters,
    })
}

/// Remove duplicate copies, keeping the first file of each cluster.
///
/// With `commit` false nothing is deleted and the outcome lists what would go.
pub fn delete_duplicates(summary: &DuplicateSummary, commit: bool) -> CleanOutcome {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    let mut reclaimed = 0u64;

    for cluster in &summary.clusters {
        kept.push(cluster.original.clone());
        for dup in &cluster.duplicates {
            if !commit {
                removed.push(dup.clone());
                reclaimed += cluster.file_size_bytes;
                continue;
            }
            if std::fs::remove_file(dup).is_ok() {
                removed.push(dup.clone());
                reclaimed += cluster.file_size_bytes;
            }
        }
    }

    CleanOutcome {
        committed: commit,
        removed_count: removed.len(),
        removed,
        reclaimed_bytes: reclaimed,
        reclaimed_human: format_bytes(reclaimed),
        kept,
    }
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
        p.push(format!("savehorse-scout-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn catalog_lists_files_with_relative_paths() {
        let dir = scratch("catalog");
        fs::write(dir.join("one.txt"), b"hello").unwrap();
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("two.txt"), b"world!!").unwrap();

        let entries = build_catalog(&dir, &WalkOptions::default()).unwrap();
        assert_eq!(entries.len(), 2);
        let names: Vec<&str> = entries.iter().map(|e| e.relative_path.as_str()).collect();
        assert_eq!(names, vec!["one.txt", "sub/two.txt"]);
        assert_eq!(entries[0].size_bytes, 5);
        assert_eq!(entries[0].extension, "txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_totals_add_up() {
        let dir = scratch("total");
        fs::write(dir.join("a"), b"12345").unwrap();
        fs::write(dir.join("b"), b"1234567890").unwrap();
        let entries = build_catalog(&dir, &WalkOptions::default()).unwrap();
        assert_eq!(catalog_total_bytes(&entries), 15);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_json_that_parses_back() {
        let dir = scratch("json");
        fs::write(dir.join("a.txt"), b"x").unwrap();
        let entries = build_catalog(&dir, &WalkOptions::default()).unwrap();
        let json = render_catalog(&entries, ExportFormat::Json).unwrap();
        let parsed: Vec<CatalogEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entries);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn renders_every_format_without_error() {
        let dir = scratch("formats");
        fs::write(dir.join("a.txt"), b"x").unwrap();
        let entries = build_catalog(&dir, &WalkOptions::default()).unwrap();
        for f in [
            ExportFormat::Table,
            ExportFormat::Json,
            ExportFormat::Csv,
            ExportFormat::Markdown,
            ExportFormat::Html,
        ] {
            let out = render_catalog(&entries, f).unwrap();
            assert!(!out.is_empty(), "{} produced nothing", f.name());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn format_parsing_accepts_aliases_and_rejects_nonsense() {
        assert_eq!(ExportFormat::parse("md").unwrap(), ExportFormat::Markdown);
        assert_eq!(ExportFormat::parse("JSON").unwrap(), ExportFormat::Json);
        assert!(ExportFormat::parse("yaml").is_err());
    }

    #[test]
    fn finds_duplicates_by_content_not_name() {
        let dir = scratch("dupes");
        fs::write(dir.join("a.txt"), b"identical").unwrap();
        fs::write(dir.join("b.txt"), b"identical").unwrap();
        fs::write(dir.join("c.txt"), b"different").unwrap();

        let summary = find_duplicates(&dir, 1, None, None).unwrap();
        assert_eq!(summary.total_clusters, 1);
        assert_eq!(summary.total_duplicate_files, 1);
        assert_eq!(summary.total_wasted_bytes, 9);
        assert_eq!(summary.clusters[0].count, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_size_different_content_is_not_a_duplicate() {
        let dir = scratch("notdupes");
        fs::write(dir.join("a.txt"), b"aaaa").unwrap();
        fs::write(dir.join("b.txt"), b"bbbb").unwrap();
        let summary = find_duplicates(&dir, 1, None, None).unwrap();
        assert_eq!(summary.total_clusters, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_deletes_nothing() {
        let dir = scratch("preview");
        fs::write(dir.join("a.txt"), b"same").unwrap();
        fs::write(dir.join("b.txt"), b"same").unwrap();

        let summary = find_duplicates(&dir, 1, None, None).unwrap();
        let outcome = delete_duplicates(&summary, false);
        assert!(!outcome.committed);
        assert_eq!(outcome.removed_count, 1);
        assert_eq!(outcome.reclaimed_bytes, 4);
        assert!(dir.join("a.txt").exists() && dir.join("b.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_deletes_the_duplicate_and_keeps_the_original() {
        let dir = scratch("commit");
        fs::write(dir.join("a.txt"), b"same").unwrap();
        fs::write(dir.join("b.txt"), b"same").unwrap();

        let summary = find_duplicates(&dir, 1, None, None).unwrap();
        let outcome = delete_duplicates(&summary, true);
        assert_eq!(outcome.removed_count, 1);
        let survivors = fs::read_dir(&dir).unwrap().count();
        assert_eq!(survivors, 1, "exactly one copy must remain");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn min_size_filters_small_files() {
        let dir = scratch("minsize");
        fs::write(dir.join("a.txt"), b"xx").unwrap();
        fs::write(dir.join("b.txt"), b"xx").unwrap();
        let summary = find_duplicates(&dir, 100, None, None).unwrap();
        assert_eq!(summary.total_clusters, 0);
        let _ = fs::remove_dir_all(&dir);
    }
}
