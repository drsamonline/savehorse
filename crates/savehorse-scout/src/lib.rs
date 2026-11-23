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
use std::fs;
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
// Space, inventory and comparison (from strata)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TreemapItem {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub percentage: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TreemapReport {
    pub root_path: String,
    pub total_size_bytes: u64,
    pub total_items: usize,
    pub items: Vec<TreemapItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InventoryReport {
    pub root_directory: String,
    pub total_files: usize,
    pub total_directories: usize,
    pub total_bytes: u64,
    pub max_directory_depth: usize,
    pub largest_file_name: String,
    pub largest_file_size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    pub file_a: String,
    pub file_b: String,
    pub size_a: u64,
    pub size_b: u64,
    pub is_identical: bool,
    pub first_difference_offset: Option<u64>,
    pub differing_bytes_count: u64,
}

fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                total += dir_size(&path);
            } else if let Ok(meta) = path.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

/// Size of each entry directly inside a directory, biggest first.
///
/// A directory's size is the sum of everything under it, which is what makes a
/// treemap readable: the big rectangles are the big subtrees.
pub fn analyze_treemap(root: &Path) -> Result<TreemapReport> {
    if !root.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            root.display()
        )));
    }

    let mut items = Vec::new();
    let mut total = 0u64;

    for entry in fs::read_dir(root)?.flatten() {
        let path = entry.path();
        let is_dir = path.is_dir();
        let size = if is_dir {
            dir_size(&path)
        } else {
            path.metadata().map(|m| m.len()).unwrap_or(0)
        };
        total += size;

        items.push(TreemapItem {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: path.to_string_lossy().to_string(),
            is_dir,
            size_bytes: size,
            percentage: 0.0,
        });
    }

    for item in items.iter_mut() {
        item.percentage = if total > 0 {
            ((item.size_bytes as f64 / total as f64) * 10000.0).round() / 100.0
        } else {
            0.0
        };
    }
    items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));

    Ok(TreemapReport {
        root_path: root.to_string_lossy().to_string(),
        total_size_bytes: total,
        total_items: items.len(),
        items,
    })
}

/// Draw a treemap as SVG: one column per item, height proportional to size.
///
/// A squarified layout is prettier and much more code; a proportional column
/// chart is honest about the same numbers and fits on one screen.
pub fn render_treemap_svg(report: &TreemapReport, width: u32) -> String {
    const HEIGHT: u32 = 420;
    let width = width.max(320);
    let mut out = String::new();

    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">\n",
        w = width,
        h = HEIGHT
    ));
    out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#0b0d10\"/>\n");

    let count = report.items.len().max(1) as u32;
    let slot = width / count;
    let scale = (HEIGHT - 60) as f64;

    for (i, item) in report.items.iter().enumerate() {
        let bar_height = (scale * (item.percentage / 100.0)).max(2.0);
        let x = i as u32 * slot;
        let y = HEIGHT - 40 - bar_height as u32;
        let colour = if item.is_dir { "#d8ff4f" } else { "#5ac8fa" };

        out.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{:.0}\" fill=\"{}\" opacity=\"0.85\"/>\n",
            x + 4,
            y,
            slot.saturating_sub(8),
            bar_height,
            colour
        ));
        out.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-family=\"monospace\" font-size=\"10\" fill=\"#98a2b0\">{}</text>\n",
            x + 6,
            HEIGHT - 24,
            escape_xml(&truncate(&item.name, 14))
        ));
    }

    out.push_str(&format!(
        "<text x=\"12\" y=\"24\" font-family=\"monospace\" font-size=\"12\" fill=\"#e9edf2\">{} — {}</text>\n",
        escape_xml(&report.root_path),
        format_bytes(report.total_size_bytes)
    ));
    out.push_str("</svg>\n");
    out
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", head)
    }
}

/// Counts, depth and the largest file in a tree.
pub fn inspect_directory(root: &Path) -> Result<InventoryReport> {
    if !root.is_dir() {
        return Err(Error::Invalid(format!(
            "'{}' is not a directory",
            root.display()
        )));
    }

    let files = savehorse_core::walk_files(
        root,
        &WalkOptions {
            include_hidden: true,
            ..Default::default()
        },
    )?;

    let mut total_bytes = 0u64;
    let mut largest_name = String::new();
    let mut largest_size = 0u64;
    let mut max_depth = 0usize;
    let mut directories = 0usize;

    for file in &files {
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += size;

        if size > largest_size {
            largest_size = size;
            largest_name = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
        }

        if let Ok(rel) = file.strip_prefix(root) {
            max_depth = max_depth.max(rel.components().count());
        }
    }

    // Directories, counted separately because walk_files only returns files.
    fn count_dirs(dir: &Path, seen: &mut usize) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    *seen += 1;
                    count_dirs(&path, seen);
                }
            }
        }
    }
    count_dirs(root, &mut directories);

    Ok(InventoryReport {
        root_directory: root.to_string_lossy().to_string(),
        total_files: files.len(),
        total_directories: directories,
        total_bytes,
        max_directory_depth: max_depth,
        largest_file_name: largest_name,
        largest_file_size: largest_size,
    })
}

/// Compare two files byte for byte.
pub fn diff_files(a: &Path, b: &Path) -> Result<DiffReport> {
    let left = fs::read(a)?;
    let right = fs::read(b)?;

    let mut differing = 0u64;
    let mut first: Option<u64> = None;

    let shorter = left.len().min(right.len());
    for i in 0..shorter {
        if left[i] != right[i] {
            differing += 1;
            if first.is_none() {
                first = Some(i as u64);
            }
        }
    }
    // Anything past the end of the shorter file also differs.
    differing += (left.len().max(right.len()) - shorter) as u64;

    Ok(DiffReport {
        file_a: a.to_string_lossy().to_string(),
        file_b: b.to_string_lossy().to_string(),
        size_a: left.len() as u64,
        size_b: right.len() as u64,
        is_identical: differing == 0,
        first_difference_offset: first,
        differing_bytes_count: differing,
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
    fn treemap_ranks_entries_by_size() {
        let dir = scratch("treemap");
        fs::write(dir.join("big.bin"), vec![0u8; 3000]).unwrap();
        fs::write(dir.join("small.txt"), b"x").unwrap();
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("mid.bin"), vec![0u8; 1000]).unwrap();

        let report = analyze_treemap(&dir).unwrap();
        // 3000 + 1 + 1000
        assert_eq!(report.total_size_bytes, 4001);
        assert_eq!(report.items[0].name, "big.bin");
        assert!((report.items[0].percentage - 74.98).abs() < 0.1);
        let sub_item = report.items.iter().find(|i| i.name == "sub").unwrap();
        assert!(sub_item.is_dir);
        assert_eq!(sub_item.size_bytes, 1000);

        let svg = render_treemap_svg(&report, 800);
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("big.bin"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn inventory_counts_files_depth_and_the_largest() {
        let dir = scratch("inventory");
        fs::create_dir_all(dir.join("a").join("b")).unwrap();
        fs::write(dir.join("top.txt"), b"12").unwrap();
        fs::write(dir.join("a").join("b").join("deep.bin"), vec![0u8; 500]).unwrap();

        let report = inspect_directory(&dir).unwrap();
        assert_eq!(report.total_files, 2);
        assert_eq!(report.total_directories, 2);
        assert_eq!(report.total_bytes, 502);
        assert_eq!(report.max_directory_depth, 3);
        assert_eq!(report.largest_file_name, "deep.bin");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diff_finds_the_first_difference() {
        let dir = scratch("diff");
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");

        fs::write(&a, b"abcdef").unwrap();
        fs::write(&b, b"abcdef").unwrap();
        let same = diff_files(&a, &b).unwrap();
        assert!(same.is_identical);
        assert_eq!(same.first_difference_offset, None);

        fs::write(&b, b"abXdeY").unwrap();
        let differ = diff_files(&a, &b).unwrap();
        assert!(!differ.is_identical);
        assert_eq!(differ.first_difference_offset, Some(2));
        assert_eq!(differ.differing_bytes_count, 2);

        // Different lengths count as differences too.
        fs::write(&b, b"abc").unwrap();
        let shorter = diff_files(&a, &b).unwrap();
        assert_eq!(shorter.differing_bytes_count, 3);
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
