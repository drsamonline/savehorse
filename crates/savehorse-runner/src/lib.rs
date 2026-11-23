//! One implementation of every ported operation.
//!
//! The command line and the desktop app both call [`execute`]. Neither owns the
//! logic, so a fix or a behaviour change lands in both at once and they cannot
//! drift apart.

use std::collections::BTreeMap;
use std::path::PathBuf;

use savehorse_core::{Error, Result};
use savehorse_registry as registry;

/// A single request: which operation, and with what.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub module: String,
    pub op: String,
    /// Positional arguments, in the order the manifest declares them.
    pub positional: Vec<String>,
    /// Flags by name, without leading dashes.
    pub flags: BTreeMap<String, String>,
}

impl Request {
    /// Build a request for `module.op`.
    pub fn new(module: &str, op: &str) -> Self {
        Request {
            module: module.to_string(),
            op: op.to_string(),
            positional: Vec::new(),
            flags: BTreeMap::new(),
        }
    }

    /// Chainable positional argument.
    pub fn arg(mut self, value: impl Into<String>) -> Self {
        self.positional.push(value.into());
        self
    }

    /// Chainable flag.
    pub fn flag(mut self, name: &str, value: impl Into<String>) -> Self {
        self.flags.insert(name.to_string(), value.into());
        self
    }

    /// True when a switch is present and not explicitly off.
    pub fn has(&self, name: &str) -> bool {
        match self.flags.get(name) {
            None => false,
            Some(v) => v != "false" && v != "0",
        }
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(|s| s.as_str())
    }

    fn position(&self, index: usize, what: &str) -> Result<&str> {
        self.positional
            .get(index)
            .map(|s| s.as_str())
            .ok_or_else(|| Error::Invalid(format!("{} is required", what)))
    }

    fn number(&self, name: &str) -> Result<Option<usize>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) if raw.trim().is_empty() => Ok(None),
            Some(raw) => raw.trim().parse::<usize>().map(Some).map_err(|_| {
                Error::Invalid(format!("--{} wants a whole number, got '{}'", name, raw))
            }),
        }
    }

    fn size(&self, name: &str) -> Result<Option<u64>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) if raw.trim().is_empty() => Ok(None),
            Some(raw) => savehorse_core::parse_size(raw).map(Some),
        }
    }

    fn paths_from(&self, from: usize) -> Result<Vec<PathBuf>> {
        let paths: Vec<PathBuf> = self.positional.iter().skip(from).map(PathBuf::from).collect();
        if paths.is_empty() {
            return Err(Error::Invalid("at least one path is required".to_string()));
        }
        Ok(paths)
    }
}

/// What an operation produced.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// A report written for a person.
    pub text: String,
    /// The same result as JSON, for a machine or another pane.
    pub json: String,
    /// True when the operation was a preview and changed nothing.
    pub dry_run: bool,
}

impl Outcome {
    fn new(text: String, json: String) -> Self {
        Outcome {
            text,
            json,
            dry_run: false,
        }
    }

    fn preview(mut self) -> Self {
        self.dry_run = true;
        self
    }
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| Error::Invalid(format!("could not encode JSON: {}", e)))
}

/// Look up the operation and run it.
///
/// An operation that is declared but not ported returns an error naming itself,
/// rather than doing nothing quietly.
pub fn execute(req: &Request) -> Result<Outcome> {
    let op = registry::op(&req.module, &req.op).ok_or_else(|| {
        Error::Invalid(format!(
            "module '{}' has no operation '{}'",
            req.module, req.op
        ))
    })?;

    if op.status == registry::Status::Planned {
        return Err(Error::Invalid(format!(
            "{} is declared in the manifest but not ported yet",
            op.id
        )));
    }

    match op.id {
        "sift.strings" => sift_strings(req),
        "sift.search" => sift_search(req),
        "sift.replace" => sift_replace(req),
        "sift.xor" => sift_xor(req),
        "scout.catalog" => scout_catalog(req),
        "scout.duplicates" => scout_duplicates(req),
        "scout.clean" => scout_clean(req),
        other => Err(Error::Invalid(format!(
            "{} is marked implemented but has no runner",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// sift
// ---------------------------------------------------------------------------

fn sift_strings(req: &Request) -> Result<Outcome> {
    let path = PathBuf::from(req.position(0, "path")?);
    let min_len = req.number("min-len")?.unwrap_or(4);
    let entries = savehorse_sift::extract_strings(
        &path,
        min_len,
        req.has("ascii"),
        req.has("unicode"),
    )?;

    let mut text = String::new();
    for e in &entries {
        text.push_str(&format!("{}  {:<8}  {}\n", e.offset_hex, e.encoding, e.content));
    }
    text.push_str(&format!("{} string(s)\n", entries.len()));

    Ok(Outcome::new(text, to_json(&entries)?))
}

fn sift_search(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let paths = req.paths_from(1)?;

    let hits = savehorse_sift::search_paths(
        pattern,
        &paths,
        req.has("regex"),
        req.has("ignore-case"),
        req.value("glob"),
    )?;

    let mut text = String::new();
    for h in &hits {
        text.push_str(&format!("{}:{}: {}\n", h.file, h.line_number, h.content));
    }
    text.push_str(&format!("{} match(es)\n", hits.len()));

    Ok(Outcome::new(text, to_json(&hits)?))
}

fn sift_replace(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let replacement = req.position(1, "replacement")?;
    let paths = req.paths_from(2)?;
    let dry_run = req.has("dry-run");

    let outcome = savehorse_sift::replace_paths(
        pattern,
        replacement,
        &paths,
        req.has("ignore-case"),
        dry_run,
        req.has("backup"),
        req.value("glob"),
    )?;

    let mut text = String::new();
    for line in &outcome.log {
        text.push_str(line);
        text.push('\n');
    }
    if outcome.dry_run {
        text.push_str(&format!(
            "preview: {} replacement(s) in {} file(s). Re-run without --dry-run to apply.\n",
            outcome.replacements, outcome.files_modified
        ));
    } else {
        text.push_str(&format!(
            "{} replacement(s) in {} file(s).\n",
            outcome.replacements, outcome.files_modified
        ));
    }

    let mut result = Outcome::new(text, to_json(&outcome)?);
    if dry_run {
        result = result.preview();
    }
    Ok(result)
}

fn sift_xor(req: &Request) -> Result<Outcome> {
    let pattern = req.position(0, "pattern")?;
    let path = PathBuf::from(req.position(1, "path")?);
    let max_key = req.number("max-key")?.unwrap_or(255).min(255) as u8;

    let hits = savehorse_sift::xor_search(pattern, &path, max_key)?;

    let mut text = String::new();
    for h in &hits {
        text.push_str(&format!(
            "key=0x{:02x}  offset=0x{:08x}  {}\n",
            h.key, h.offset, h.context
        ));
    }
    text.push_str(&format!("{} hit(s)\n", hits.len()));

    Ok(Outcome::new(text, to_json(&hits)?))
}

// ---------------------------------------------------------------------------
// scout
// ---------------------------------------------------------------------------

fn scout_catalog(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let format = match req.value("format") {
        Some(f) if !f.trim().is_empty() => savehorse_scout::ExportFormat::parse(f)?,
        _ => savehorse_scout::ExportFormat::Table,
    };

    let opts = savehorse_core::WalkOptions {
        max_depth: req.number("depth")?,
        min_size: req.size("min-size")?,
        max_size: req.size("max-size")?,
        ext: req.value("ext").map(|s| s.to_string()),
        include_hidden: false,
    };

    let entries = savehorse_scout::build_catalog(&root, &opts)?;
    let rendered = savehorse_scout::render_catalog(&entries, format)?;
    let json = to_json(&entries)?;

    if let Some(dest) = req.value("out") {
        if !dest.trim().is_empty() {
            std::fs::write(dest, &rendered)?;
            return Ok(Outcome::new(
                format!("catalogued {} file(s) to '{}'\n", entries.len(), dest),
                json,
            ));
        }
    }

    Ok(Outcome::new(rendered, json))
}

fn scout_duplicates(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let summary = savehorse_scout::find_duplicates(
        &root,
        req.size("min-size")?.unwrap_or(1),
        req.value("ext"),
        req.number("depth")?,
    )?;

    let mut text = format!(
        "Duplicate scan of '{}':\n  clusters           : {}\n  redundant files    : {}\n  recoverable space  : {}\n",
        root.display(),
        summary.total_clusters,
        summary.total_duplicate_files,
        summary.total_wasted_human
    );
    if !summary.clusters.is_empty() {
        text.push('\n');
    }
    for (idx, cluster) in summary.clusters.iter().enumerate() {
        let short = &cluster.sha256[..16.min(cluster.sha256.len())];
        text.push_str(&format!(
            "Cluster #{}: sha256={} ({} each)\n",
            idx + 1,
            short,
            cluster.file_size_human
        ));
        text.push_str(&format!("  keep      {}\n", cluster.original));
        for dup in &cluster.duplicates {
            text.push_str(&format!("  duplicate {}\n", dup));
        }
        text.push_str(&format!(
            "  wasted    {}\n\n",
            savehorse_core::format_bytes(cluster.wasted_bytes)
        ));
    }

    Ok(Outcome::new(text, to_json(&summary)?))
}

fn scout_clean(req: &Request) -> Result<Outcome> {
    let root = PathBuf::from(req.position(0, "path")?);
    let commit = req.has("commit");

    let summary =
        savehorse_scout::find_duplicates(&root, req.size("min-size")?.unwrap_or(1), req.value("ext"), None)?;
    let outcome = savehorse_scout::delete_duplicates(&summary, commit);

    let mut text = String::new();
    if !commit {
        text.push_str("Preview: nothing has been deleted.\n");
        for path in &outcome.removed {
            text.push_str(&format!("  would remove {}\n", path));
        }
        text.push_str(&format!(
            "\n{} file(s), {} reclaimable. Re-run with --commit to delete.\n",
            outcome.removed_count, outcome.reclaimed_human
        ));
    } else {
        for path in &outcome.removed {
            text.push_str(&format!("  removed {}\n", path));
        }
        text.push_str(&format!(
            "\n{} file(s) removed, {} reclaimed.\n",
            outcome.removed_count, outcome.reclaimed_human
        ));
    }

    let mut result = Outcome::new(text, to_json(&outcome)?);
    if !commit {
        result = result.preview();
    }
    Ok(result)
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
        p.push(format!("savehorse-runner-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn planned_operations_refuse_to_run() {
        let req = Request::new("aegis", "shred").arg("/tmp/whatever");
        let err = execute(&req).unwrap_err().to_string();
        assert!(err.contains("not ported yet"), "got: {}", err);
    }

    #[test]
    fn unknown_operations_name_themselves() {
        let req = Request::new("sift", "teleport");
        let err = execute(&req).unwrap_err().to_string();
        assert!(err.contains("no operation 'teleport'"), "got: {}", err);
    }

    #[test]
    fn search_returns_text_and_json() {
        let dir = scratch("search");
        let file = dir.join("a.txt");
        fs::write(&file, "needle here\nnothing\n").unwrap();

        let req = Request::new("sift", "search")
            .arg("needle")
            .arg(file.to_string_lossy().to_string());
        let out = execute(&req).unwrap();

        assert!(out.text.contains("1 match(es)"), "got: {}", out.text);
        assert!(out.json.contains("needle here"));
        assert!(!out.dry_run);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_marks_itself_as_a_preview() {
        let dir = scratch("replace");
        let file = dir.join("a.txt");
        fs::write(&file, "cat").unwrap();

        let req = Request::new("sift", "replace")
            .arg("cat")
            .arg("dog")
            .arg(file.to_string_lossy().to_string())
            .flag("dry-run", "true");
        let out = execute(&req).unwrap();

        assert!(out.dry_run, "a dry run must report itself as a preview");
        assert_eq!(fs::read_to_string(&file).unwrap(), "cat");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_without_commit_is_a_preview() {
        let dir = scratch("clean");
        fs::write(dir.join("a.txt"), b"same").unwrap();
        fs::write(dir.join("b.txt"), b"same").unwrap();

        let req = Request::new("scout", "clean").arg(dir.to_string_lossy().to_string());
        let out = execute(&req).unwrap();

        assert!(out.dry_run);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_renders_the_requested_format() {
        let dir = scratch("catalog");
        fs::write(dir.join("a.txt"), b"x").unwrap();

        let req = Request::new("scout", "catalog")
            .arg(dir.to_string_lossy().to_string())
            .flag("format", "json");
        let out = execute(&req).unwrap();

        let parsed: Vec<savehorse_scout::CatalogEntry> = serde_json::from_str(&out.text).unwrap();
        assert_eq!(parsed.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
