//! The savehorse command line.
//!
//! `savehorse <module> <op> [arguments]`
//!
//! Every operation is looked up in `shavehorse-registry` before anything runs,
//! so an operation that is declared but not yet ported says so plainly instead
//! of failing somewhere deeper.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use shavehorse_core::{parse_size, Error, Result};
use shavehorse_registry as registry;

/// Flags that never take a value. Anything else is treated as `--key value`.
const BOOL_FLAGS: &[&str] = &[
    "json",
    "dry-run",
    "commit",
    "backup",
    "regex",
    "ignore-case",
    "ascii",
    "unicode",
    "help",
    "version",
];

/// One parsed command line.
struct Invocation {
    module: String,
    op: String,
    positional: Vec<String>,
    flags: HashMap<String, String>,
}

impl Invocation {
    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(|s| s.as_str())
    }

    fn position(&self, index: usize, what: &str) -> Result<&str> {
        self.positional.get(index).map(|s| s.as_str()).ok_or_else(|| {
            Error::Invalid(format!(
                "{} is required: savehorse {} {} <{}>",
                what, self.module, self.op, what
            ))
        })
    }

    fn number(&self, name: &str) -> Result<Option<usize>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) => raw
                .trim()
                .parse::<usize>()
                .map(Some)
                .map_err(|_| Error::Invalid(format!("--{} wants a whole number, got '{}'", name, raw))),
        }
    }

    fn size(&self, name: &str) -> Result<Option<u64>> {
        match self.value(name) {
            None => Ok(None),
            Some(raw) => parse_size(raw).map(Some),
        }
    }

    /// Every positional from `from` onwards, as paths.
    fn paths_from(&self, from: usize) -> Result<Vec<PathBuf>> {
        let paths: Vec<PathBuf> = self
            .positional
            .iter()
            .skip(from)
            .map(PathBuf::from)
            .collect();
        if paths.is_empty() {
            return Err(Error::Invalid(format!(
                "at least one path is required: savehorse {} {} ... <path>",
                self.module, self.op
            )));
        }
        Ok(paths)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("savehorse: {}", e);
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();

    // If the binary was invoked under a module's own name (a symlink called
    // `sift`, say), treat that name as the module. This is what keeps the old
    // `sift search ...` muscle memory working.
    let argv0 = std::env::args().next().unwrap_or_default();
    let argv0_path = PathBuf::from(&argv0);
    if let Some(stem) = argv0_path.file_stem().and_then(|s| s.to_str()) {
        if stem != "savehorse" && registry::module(stem).is_some() {
            let module = stem.to_string();
            args.insert(0, module);
        }
    }

    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print_help();
        return Ok(ExitCode::SUCCESS);
    }

    if args[0] == "--version" || args[0] == "-V" {
        println!("savehorse {}", env!("CARGO_PKG_VERSION"));
        let (total, implemented) = registry::counts();
        println!("{} operations declared, {} implemented", total, implemented);
        return Ok(ExitCode::SUCCESS);
    }

    if args[0] == "ops" {
        print_ops(args.get(1).map(|s| s.as_str()));
        return Ok(ExitCode::SUCCESS);
    }

    if args[0] == "modules" {
        print_modules();
        return Ok(ExitCode::SUCCESS);
    }

    let module_name = args[0].clone();
    if registry::module(&module_name).is_none() {
        return Err(Error::Invalid(format!(
            "unknown module '{}'. Run 'savehorse modules' to see them all.",
            module_name
        )));
    }

    if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
        print_module_help(&module_name);
        return Ok(ExitCode::SUCCESS);
    }

    let op_name = args[1].clone();
    let (positional, flags) = parse_arguments(&args[2..])?;

    let invocation = Invocation {
        module: module_name,
        op: op_name,
        positional,
        flags,
    };

    dispatch(&invocation)
}

/// Split the tail of the command line into positionals and flags.
fn parse_arguments(args: &[String]) -> Result<(Vec<String>, HashMap<String, String>)> {
    let mut positional = Vec::new();
    let mut flags = HashMap::new();
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if let Some(body) = arg.strip_prefix("--") {
            if let Some(eq) = body.find('=') {
                let key = body[..eq].to_string();
                let value = body[eq + 1..].to_string();
                flags.insert(key, value);
                i += 1;
                continue;
            }

            if BOOL_FLAGS.contains(&body) {
                flags.insert(body.to_string(), "true".to_string());
                i += 1;
                continue;
            }

            // A valued flag: take the next argument unless it looks like
            // another flag or there is nothing left.
            match args.get(i + 1) {
                Some(next) if !next.starts_with("--") => {
                    flags.insert(body.to_string(), next.clone());
                    i += 2;
                }
                _ => {
                    return Err(Error::Invalid(format!("--{} needs a value", body)));
                }
            }
            continue;
        }

        positional.push(arg.clone());
        i += 1;
    }

    Ok((positional, flags))
}

fn dispatch(inv: &Invocation) -> Result<ExitCode> {
    let op = registry::op(&inv.module, &inv.op).ok_or_else(|| {
        Error::Invalid(format!(
            "module '{}' has no operation '{}'. Run 'savehorse {} --help'.",
            inv.module, inv.op, inv.module
        ))
    })?;

    if op.status == registry::Status::Planned {
        return Err(Error::Invalid(format!(
            "{} is declared in the manifest but not ported yet",
            op.id
        )));
    }

    match op.id {
        "sift.strings" => sift_strings(inv),
        "sift.search" => sift_search(inv),
        "sift.replace" => sift_replace(inv),
        "sift.xor" => sift_xor(inv),
        "scout.catalog" => scout_catalog(inv),
        "scout.duplicates" => scout_duplicates(inv),
        "scout.clean" => scout_clean(inv),
        other => Err(Error::Invalid(format!(
            "{} is marked implemented but has no dispatch arm",
            other
        ))),
    }
}

// ---------------------------------------------------------------------------
// sift
// ---------------------------------------------------------------------------

fn sift_strings(inv: &Invocation) -> Result<ExitCode> {
    let path = PathBuf::from(inv.position(0, "path")?);
    let min_len = inv.number("min-len")?.unwrap_or(4);
    let ascii_only = inv.has("ascii");
    let unicode_only = inv.has("unicode");

    let entries = shavehorse_sift::extract_strings(&path, min_len, ascii_only, unicode_only)?;

    if inv.has("json") {
        println!("{}", encode(&entries)?);
    } else {
        for e in &entries {
            println!("{}  {:<8}  {}", e.offset_hex, e.encoding, e.content);
        }
        println!("{} string(s)", entries.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn sift_search(inv: &Invocation) -> Result<ExitCode> {
    let pattern = inv.position(0, "pattern")?;
    let paths = inv.paths_from(1)?;
    let is_regex = inv.has("regex");
    let ignore_case = inv.has("ignore-case");
    let glob = inv.value("glob");

    let hits = shavehorse_sift::search_paths(pattern, &paths, is_regex, ignore_case, glob)?;

    if inv.has("json") {
        println!("{}", encode(&hits)?);
    } else {
        for h in &hits {
            println!("{}:{}: {}", h.file, h.line_number, h.content);
        }
        println!("{} match(es)", hits.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn sift_replace(inv: &Invocation) -> Result<ExitCode> {
    let pattern = inv.position(0, "pattern")?;
    let replacement = inv.position(1, "replacement")?;
    let paths = inv.paths_from(2)?;
    let ignore_case = inv.has("ignore-case");
    let dry_run = inv.has("dry-run");
    let backup = inv.has("backup");
    let glob = inv.value("glob");

    let outcome = shavehorse_sift::replace_paths(
        pattern,
        replacement,
        &paths,
        ignore_case,
        dry_run,
        backup,
        glob,
    )?;

    if inv.has("json") {
        println!("{}", encode(&outcome)?);
    } else {
        for line in &outcome.log {
            println!("{}", line);
        }
        if outcome.dry_run {
            println!(
                "preview: {} replacement(s) in {} file(s). Re-run without --dry-run to apply.",
                outcome.replacements, outcome.files_modified
            );
        } else {
            println!(
                "{} replacement(s) in {} file(s).",
                outcome.replacements, outcome.files_modified
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn sift_xor(inv: &Invocation) -> Result<ExitCode> {
    let pattern = inv.position(0, "pattern")?;
    let path = PathBuf::from(inv.position(1, "path")?);
    let max_key = inv.number("max-key")?.unwrap_or(255).min(255) as u8;

    let hits = shavehorse_sift::xor_search(pattern, &path, max_key)?;

    if inv.has("json") {
        println!("{}", encode(&hits)?);
    } else {
        for h in &hits {
            println!("key=0x{:02x}  offset=0x{:08x}  {}", h.key, h.offset, h.context);
        }
        println!("{} hit(s)", hits.len());
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// scout
// ---------------------------------------------------------------------------

fn scout_catalog(inv: &Invocation) -> Result<ExitCode> {
    let root = PathBuf::from(inv.position(0, "path")?);
    let format = match inv.value("format") {
        Some(f) => shavehorse_scout::ExportFormat::parse(f)?,
        None => shavehorse_scout::ExportFormat::Table,
    };

    let opts = shavehorse_core::WalkOptions {
        max_depth: inv.number("depth")?,
        min_size: inv.size("min-size")?,
        max_size: inv.size("max-size")?,
        ext: inv.value("ext").map(|s| s.to_string()),
        include_hidden: false,
    };

    let entries = shavehorse_scout::build_catalog(&root, &opts)?;
    let rendered = shavehorse_scout::render_catalog(&entries, format)?;

    match inv.value("out") {
        Some(dest) => {
            std::fs::write(dest, &rendered)?;
            println!(
                "catalogued {} file(s) to '{}'",
                entries.len(),
                dest
            );
        }
        None => {
            // The JSON and CSV renderers do not end with a newline; without
            // this the next thing printed lands on the same line.
            if rendered.ends_with('\n') {
                print!("{}", rendered);
            } else {
                println!("{}", rendered);
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn scout_duplicates(inv: &Invocation) -> Result<ExitCode> {
    let root = PathBuf::from(inv.position(0, "path")?);
    let min_size = inv.size("min-size")?.unwrap_or(1);
    let ext = inv.value("ext");
    let depth = inv.number("depth")?;

    let summary = shavehorse_scout::find_duplicates(&root, min_size, ext, depth)?;

    if inv.has("json") {
        println!("{}", encode(&summary)?);
    } else {
        println!("Duplicate scan of '{}':", root.display());
        println!("  clusters           : {}", summary.total_clusters);
        println!("  redundant files    : {}", summary.total_duplicate_files);
        println!("  recoverable space  : {}", summary.total_wasted_human);
        if !summary.clusters.is_empty() {
            println!();
        }
        for (idx, cluster) in summary.clusters.iter().enumerate() {
            let short = &cluster.sha256[..16.min(cluster.sha256.len())];
            println!(
                "Cluster #{}: sha256={} ({} each)",
                idx + 1,
                short,
                cluster.file_size_human
            );
            println!("  keep      {}", cluster.original);
            for dup in &cluster.duplicates {
                println!("  duplicate {}", dup);
            }
            println!("  wasted    {}", shavehorse_core::format_bytes(cluster.wasted_bytes));
            println!();
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn scout_clean(inv: &Invocation) -> Result<ExitCode> {
    let root = PathBuf::from(inv.position(0, "path")?);
    let commit = inv.has("commit");
    let min_size = inv.size("min-size")?.unwrap_or(1);
    let ext = inv.value("ext");

    let summary = shavehorse_scout::find_duplicates(&root, min_size, ext, None)?;
    let outcome = shavehorse_scout::delete_duplicates(&summary, commit);

    if inv.has("json") {
        println!("{}", encode(&outcome)?);
    } else if !commit {
        println!("Preview: nothing has been deleted.");
        for path in &outcome.removed {
            println!("  would remove {}", path);
        }
        println!(
            "\n{} file(s), {} reclaimable. Re-run with --commit to delete.",
            outcome.removed_count, outcome.reclaimed_human
        );
    } else {
        for path in &outcome.removed {
            println!("  removed {}", path);
        }
        println!(
            "\n{} file(s) removed, {} reclaimed.",
            outcome.removed_count, outcome.reclaimed_human
        );
    }

    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// Output helpers
// ---------------------------------------------------------------------------

fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| Error::Invalid(format!("could not encode JSON: {}", e)))
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

fn print_help() {
    let (total, implemented) = registry::counts();
    println!("savehorse {}", env!("CARGO_PKG_VERSION"));
    println!("One bench, thirteen tools. {} operations, {} ported so far.", total, implemented);
    println!();
    println!("USAGE");
    println!("  savehorse <module> <op> [arguments] [flags]");
    println!("  savehorse modules                list the modules");
    println!("  savehorse ops [module]           list operations");
    println!("  savehorse <module> --help        what a module can do");
    println!();
    println!("GLOBAL FLAGS");
    println!("  --json          machine-readable output");
    println!("  --dry-run       preview, change nothing");
    println!("  --commit        apply a destructive operation");
    println!();
    println!("MODULES");
    for m in registry::MODULES {
        let ops = registry::ops_in(m.name);
        let done = ops
            .iter()
            .filter(|o| o.status == registry::Status::Implemented)
            .count();
        println!(
            "  {:<8} {:<18} {:<38} {}/{} ported",
            m.name,
            m.title,
            m.job,
            done,
            ops.len()
        );
    }
    println!();
    println!("Start with: savehorse sift --help");
}

fn print_modules() {
    for m in registry::MODULES {
        let ops = registry::ops_in(m.name);
        println!("{} — {}", m.name, m.title);
        println!("  job     {}", m.job);
        println!("  from    {}", m.sources);
        println!("  ops     {}", ops.len());
        println!();
    }
}

fn print_ops(only_module: Option<&str>) {
    for m in registry::MODULES {
        if let Some(name) = only_module {
            if m.name != name {
                continue;
            }
        }
        println!("{} — {}", m.name, m.title);
        for o in registry::ops_in(m.name) {
            let mark = match o.status {
                registry::Status::Implemented => "ready",
                registry::Status::Planned => "planned",
            };
            println!(
                "  {:<22} {:<8} {:<12} {}",
                o.id,
                mark,
                o.danger.label(),
                o.summary
            );
        }
        println!();
    }
}

fn print_module_help(module_name: &str) {
    let m = match registry::module(module_name) {
        Some(m) => m,
        None => return,
    };
    println!("{} — {}", m.name, m.title);
    println!("{}", m.job);
    println!("Replaces: {}", m.sources);
    println!();
    for o in registry::ops_in(m.name) {
        let mark = match o.status {
            registry::Status::Implemented => "",
            registry::Status::Planned => "  (planned)",
        };
        println!(
            "  savehorse {} {}{}",
            m.name, o.name, mark
        );
        println!("      {} [{}]", o.summary, o.danger.label());
        for p in o.params {
            let req = if p.required { "required" } else { "optional" };
            let pos = if p.positional { "positional" } else { "flag" };
            println!("      {:<14} {:<10} {:<10} {}", p.name, pos, req, p.help);
        }
        println!();
    }
}
