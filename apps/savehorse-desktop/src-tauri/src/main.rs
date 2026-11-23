//! The savehorse desktop shell.
//!
//! Two commands, and that is the whole surface: one hands the interface the
//! operation manifest, the other runs an operation. Everything the window can
//! do, the command line can do, because both call `savehorse-runner`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use savehorse_registry as registry;

#[derive(Serialize)]
struct ParamView {
    name: String,
    kind: String,
    positional: bool,
    required: bool,
    help: String,
    options: Vec<String>,
}

#[derive(Serialize)]
struct OpView {
    id: String,
    name: String,
    summary: String,
    danger: String,
    status: String,
    params: Vec<ParamView>,
}

#[derive(Serialize)]
struct ModuleView {
    name: String,
    title: String,
    job: String,
    sources: String,
    ops: Vec<OpView>,
}

fn danger_key(d: registry::Danger) -> &'static str {
    match d {
        registry::Danger::Safe => "safe",
        registry::Danger::Destructive => "destructive",
        registry::Danger::Irreversible => "irreversible",
    }
}

fn status_key(s: registry::Status) -> &'static str {
    match s {
        registry::Status::Implemented => "implemented",
        registry::Status::Planned => "planned",
    }
}

fn param_kind(k: registry::ParamKind) -> &'static str {
    match k {
        registry::ParamKind::Path => "Path",
        registry::ParamKind::Paths => "Paths",
        registry::ParamKind::Text => "Text",
        registry::ParamKind::Pattern => "Pattern",
        registry::ParamKind::Number => "Number",
        registry::ParamKind::Size => "Size",
        registry::ParamKind::Bool => "Bool",
        registry::ParamKind::Choice => "Choice",
        registry::ParamKind::Secret => "Secret",
    }
}

/// The whole manifest, shaped for the interface.
///
/// The window renders its forms from this. It is the same declaration the CLI
/// parses, so a new operation appears in both without a second edit.
#[tauri::command]
fn manifest() -> Vec<ModuleView> {
    registry::MODULES
        .iter()
        .map(|m| ModuleView {
            name: m.name.to_string(),
            title: m.title.to_string(),
            job: m.job.to_string(),
            sources: m.sources.to_string(),
            ops: registry::ops_in(m.name)
                .iter()
                .map(|o| OpView {
                    id: o.id.to_string(),
                    name: o.name.to_string(),
                    summary: o.summary.to_string(),
                    danger: danger_key(o.danger).to_string(),
                    status: status_key(o.status).to_string(),
                    params: o
                        .params
                        .iter()
                        .map(|p| ParamView {
                            name: p.name.to_string(),
                            kind: param_kind(p.kind).to_string(),
                            positional: p.positional,
                            required: p.required,
                            help: p.help.to_string(),
                            options: p.options.iter().map(|s| s.to_string()).collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}

#[derive(Deserialize)]
struct RunRequest {
    module: String,
    op: String,
    positional: Vec<String>,
    flags: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct RunResult {
    text: String,
    json: String,
    dry_run: bool,
}

/// Run one operation through the shared runner.
#[tauri::command]
fn run_operation(request: RunRequest) -> Result<RunResult, String> {
    let req = savehorse_runner::Request {
        module: request.module,
        op: request.op,
        positional: request.positional,
        flags: request.flags,
    };

    savehorse_runner::execute(&req)
        .map(|outcome| RunResult {
            text: outcome.text,
            json: outcome.json,
            dry_run: outcome.dry_run,
        })
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![manifest, run_operation])
        .run(tauri::generate_context!())
        .expect("savehorse failed to start");
}
