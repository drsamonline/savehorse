# Decisions

Short records of the choices that are expensive to reverse. Each one names the
alternatives that were rejected and why.

---

## 0001 — One monorepo, not thirteen

**Decision.** All thirteen modules live in one Cargo workspace with one shared
core crate.

**Why.** The twenty-two original applications shared one architecture (`core.rs`
plus a CLI plus a GUI), one dependency set and one visual style. Thirteen
separate repositories would mean thirteen places to bump the shared crate,
thirteen CI configurations and thirteen release processes for what is one
product with one installer.

**Rejected.** One repository per module — no shared code, no single release.
One repository per generation — preserves the duplication the project exists to
remove.

**Revisit if** a module grows into a genuinely separate product. `pager` is the
only candidate, because it needs a resident background agent.

---

## 0002 — Port the logic, replace the GUI

**Decision.** Keep the Rust domain logic. Throw away the 15,000 lines of
hand-written Win32 window code and build one desktop shell.

**Why.** The cores are the valuable part and 18 of 22 of them contain no Windows
API calls at all. The GUI layer, by contrast, re-implements the same
`CreateWindowExW` boilerplate in all twenty-two applications, works only on
Windows, and cannot be themed or tested.

**Rejected.** Modernising the Win32 code — keeps the duplication, keeps the
platform lock, and a twenty-third copy of the same window code would arrive with
the first new module.

---

## 0003 — Tauri 2 with a React front end

**Decision.** The desktop app is Tauri 2 (Rust backend) with React, TypeScript
and Vite in the webview. The interface is built on a hand-written token layer
and plain CSS — no Tailwind, no component library.

**Why.** The domain logic is already Rust, so Tauri reuses it as library crates
with no rewrite. Installers land around 6 MB rather than the 120 MB an Electron
build needs. WebView2 ships with Windows 10 and 11; macOS and Linux use the
system webview. The front end is where a distinctive interface is cheapest to
build and iterate.

**Rejected.** `egui` and `iced` — pure Rust and single-binary, but constrained
styling, and the brief calls for an interface that does not look like every
other Rust tool. Electron — no Rust reuse, large installers.

**On Tailwind.** The plan named it; the build does not use it. A utility
framework earns its keep on a large, changing surface, and this is one window
with a fixed vocabulary of a dozen components. Plain CSS with tokens is fewer
moving parts, produces 11 kB of stylesheet instead of a framework runtime, and
keeps the design decisions in one readable file.

**Cost accepted.** A webview dependency, and `webkit2gtk-4.1` on Linux. If a
single dependency-free binary ever matters more than the interface, `egui` is
the fallback and the core crates do not change.

---

## 0004 — Cross-platform, with honest exceptions

**Decision.** Windows, macOS and Linux from the start.

**Why.** Eighteen of twenty-two cores are already portable. The four that are
not — `beacon`, `dock`, `pager`, `pulse` — are the system and window tools, and
their portability is a known, bounded problem.

**Consequence.** `pager` is Windows-only in v1 and says so. `pulse` moves to the
cross-platform `sysinfo` crate rather than hand-rolled Win32 calls.

---

## 0005 — The operation manifest is the source of truth

**Decision.** Every operation is declared once, in
`crates/savehorse-registry`, with its parameters, danger level and status.

**Why.** Eighty-eight operations would otherwise mean eighty-eight hand-written
argument parsers, eighty-eight hand-written forms and eighty-eight places for
the documentation to drift. Declaring them once gives the CLI its parser and
help text, the desktop app its forms and validation, the docs their tables, and
the tests a checklist. A unit test asserts the count.

**Consequence.** Adding an operation means adding a manifest entry first. That
friction is the point.

---

## 0006 — Tests before the big move

**Decision.** Only two modules are ported in the first push, and both carry
tests.

**Why.** There is not one `#[test]` in the original 29,630 lines, and the CI
ran `cargo test` against an empty suite. Porting eleven more modules onto an
untested base would hide behaviour changes in noise. `sift` and `scout` exist to
prove the pattern — the crate shape, the error type, the test harness and the
CLI dispatch — before it is repeated eleven times.

**Consequence.** The remaining 81 operations are declared but refuse to run,
naming themselves. Nothing is silently missing.

---

## 0007 — PolyForm Noncommercial 1.0.0, for now

**Decision.** Keep the licence the original applications shipped with.

**Why.** It is what the portfolio already declares, and relicensing is a
decision with consequences that should be made deliberately rather than as a
side effect of a refactor.

**Open.** PolyForm Noncommercial forbids commercial use, which includes selling
the application or building a paid tier on it. If either is ever intended, this
must change to MIT or Apache-2.0 **before** the first public release, because
permission already granted cannot be withdrawn.

---

## 0008 — One runner, two front ends

**Decision.** The dispatch that turns a request into an outcome lives in
`savehorse-runner`, not in the CLI and not in the desktop shell.

**Why.** The first draft put it in the CLI, which would have meant copying it
into the window — two implementations of seven operations, diverging from the
first bug fix. Extracting it cost one refactor and means the GUI cannot quietly
behave differently from the command line.

**Consequence.** Neither front end owns any logic. The CLI parses arguments and
prints; the window collects form values and displays. Adding an operation means
one arm in the runner, and both get it.
