//! The savehorse command line.
//!
//! `savehorse <module> <op> [arguments] [flags]`
//!
//! This file does three things and no more: read the command line, look the
//! operation up in the manifest, and print what the runner returns. The work
//! itself lives in `savehorse-runner`, which the desktop app calls too.

use std::collections::BTreeMap;
use std::process::ExitCode;

use savehorse_core::{Error, Result};
use savehorse_registry as registry;
use savehorse_runner::{execute, Request};

/// Switches that belong to the command line rather than to any one operation.
/// Everything else that is boolean is read from the manifest, so adding a
/// switch to an operation does not mean remembering to edit this file.
const GLOBAL_SWITCHES: &[&str] = &["json", "dry-run", "help", "version"];

/// Rust ignores SIGPIPE by default, so writing to a closed pipe returns an
/// error and `println!` panics on it. That turns `savehorse ops | head` into a
/// stack trace. Restoring the default disposition makes it exit quietly, the
/// way every other command line tool does.
#[cfg(unix)]
fn restore_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();
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

    // If the binary was invoked under a module's own name — a symlink called
    // `sift`, say — treat that name as the module, so old muscle memory works.
    let argv0 = std::env::args().next().unwrap_or_default();
    let argv0_path = std::path::PathBuf::from(&argv0);
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
        let (total, implemented) = registry::counts();
        println!("savehorse {}", env!("CARGO_PKG_VERSION"));
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

    // Ask the manifest which of this operation's parameters are switches,
    // instead of keeping a second list here that can drift from it.
    let declared: &[registry::Param] = registry::op(&module_name, &args[1])
        .map(|op| op.params)
        .unwrap_or(&[]);

    let (positional, flags) = parse_arguments(&args[2..], declared)?;
    let want_json = flags.contains_key("json");

    let request = Request {
        module: module_name,
        op: args[1].clone(),
        positional,
        flags,
    };

    let outcome = execute(&request)?;
    if want_json {
        println!("{}", outcome.json);
    } else if outcome.text.ends_with('\n') || outcome.text.is_empty() {
        print!("{}", outcome.text);
    } else {
        println!("{}", outcome.text);
    }

    Ok(ExitCode::SUCCESS)
}

/// Split the tail of the command line into positionals and flags.
fn parse_arguments(
    args: &[String],
    declared: &[registry::Param],
) -> Result<(Vec<String>, BTreeMap<String, String>)> {
    let mut switches: Vec<&str> = GLOBAL_SWITCHES.to_vec();
    for param in declared {
        if param.kind == registry::ParamKind::Bool {
            switches.push(param.name);
        }
    }

    let mut positional = Vec::new();
    let mut flags = BTreeMap::new();
    let mut i = 0usize;

    while i < args.len() {
        let arg = &args[i];
        if let Some(body) = arg.strip_prefix("--") {
            if let Some(eq) = body.find('=') {
                flags.insert(body[..eq].to_string(), body[eq + 1..].to_string());
                i += 1;
                continue;
            }

            if switches.contains(&body) {
                flags.insert(body.to_string(), "true".to_string());
                i += 1;
                continue;
            }

            match args.get(i + 1) {
                Some(next) if !next.starts_with("--") => {
                    flags.insert(body.to_string(), next.clone());
                    i += 2;
                }
                _ => return Err(Error::Invalid(format!("--{} needs a value", body))),
            }
            continue;
        }

        positional.push(arg.clone());
        i += 1;
    }

    Ok((positional, flags))
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

fn print_help() {
    let (total, implemented) = registry::counts();
    println!("savehorse {}", env!("CARGO_PKG_VERSION"));
    println!(
        "One bench, thirteen tools. {} operations, {} ported so far.",
        total, implemented
    );
    println!();
    println!("USAGE");
    println!("  savehorse <module> <op> [arguments] [flags]");
    println!("  savehorse modules                list the modules");
    println!("  savehorse ops [module]           list operations");
    println!("  savehorse <module> --help        what a module can do");
    println!();
    println!("FLAGS");
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
        println!("  savehorse {} {}{}", m.name, o.name, mark);
        println!("      {} [{}]", o.summary, o.danger.label());
        for p in o.params {
            let req = if p.required { "required" } else { "optional" };
            let pos = if p.positional { "positional" } else { "flag" };
            println!("      {:<14} {:<10} {:<10} {}", p.name, pos, req, p.help);
        }
        println!();
    }
}
