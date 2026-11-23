# Savehorse — Unified Rust Tool Suite
### Consolidation, modernisation and repository plan

**Prepared for:** Dr Sohil Momin
**Date:** 23 November 2026
**Status:** Proposal for approval — no code has been written yet
**Scope:** 22 existing Rust applications (two archives) → one product

---

## 1. What the archives actually contain

I extracted and read both archives. They are not two apps — they are **two generations of a 22-app portfolio**.

| | `releases.7z` | `releases.7z.001` |
|---|---|---|
| Apps | 11 | 11 |
| Generation | Gen 1 | Gen 2 (newer) |
| App names | aegis, beacon, chronos, dock, flux, forge, glyph, optic, prism, strata, vortex | arc, halo, iris, morph, pager, pulse, revive, scout, sift, slate, vault |
| Crypto | hand-rolled LCG PRNG, hand-written CRC32 | real crates: `aes-gcm`, `pbkdf2`, `sha2`, `rand` |
| Repo hygiene | empty `.github/workflows` | CI workflow, `.gitignore`, git history, icons, screenshots |
| Layout | `app/src/*.rs` | `app/src/{Cargo.toml,src/*.rs}` + `gui_main.rs` split |
| Lines per app | ~1,100–1,260 | ~1,265–1,760 |

**Totals across both: 22 apps, 29,630 lines of Rust, ~88 CLI operations, 22 hand-written GUIs.**

### Honest quality assessment

| Dimension | Finding |
|---|---|
| Architecture | Good instinct: every app is `core.rs` (no UI) + `main.rs` (CLI) + `gui.rs` (GUI). This clean split is why consolidation is cheap. |
| Portability | **18 of 22 cores have zero Windows API calls.** Only beacon, dock, pager, pulse touch Win32 — the system and window tools, as expected. |
| GUI | ~15,000 lines of duplicated raw Win32 FFI (`CreateWindowExW`, manual `WM_COMMAND` dispatch, hand-painted layout). Every app re-implements the same boilerplate. Windows-only, fixed-pixel, dated. |
| Tests | **Zero.** Not one `#[test]` in 29,630 lines. The CI runs `cargo test` against an empty suite. |
| Gen 1 crypto | `aegis` derives keys with a custom LCG PRNG and its own CRC32. That is not cryptographic randomness. Gen 2's `vault` fixed this correctly. |
| Honesty of features | `vortex` "simulate transfer" is a simulation, not a download. `flux` synthesises real PCM, but there is no actual transcoding. Screenshots are hand-drawn SVG terminal mockups, not real UI captures. |
| Licence | PolyForm Noncommercial 1.0.0 throughout. |

**Verdict:** the domain logic is real and mostly portable. The GUI layer, the test layer and the packaging are where the work is. Consolidation is the right move — you have 22 apps that share one architecture, one dependency set and one visual style, and 15,000 lines of the same window code.

---

## 2. The overlap problem

Gen 2 re-implemented several Gen 1 apps with better internals. Left as-is, you maintain the same feature twice.

| Job to be done | Gen 1 | Gen 2 | Overlap |
|---|---|---|---|
| Encryption, shredding, threat scan | aegis | vault | aegis ⊃ vault, plus scanner + persistence audit |
| ISO / optical disc authoring | optic | halo | optic ⊃ halo, plus CUE sheets + cover art |
| Image inspect / convert / thumb | prism | iris | prism has `.ico` + contact sheet; iris has batch |
| Batch rename | strata (rename) | morph | morph has the undo journal — better |
| Duplicate detection | strata | scout | near-identical |
| File carving / recovery | chronos | revive | revive is deeper |
| Window management | dock | pager | pager is richer (rollup, switch, move) |
| System diagnostics | beacon | pulse | pulse is broader |
| Text / search / replace | glyph (partly) | sift, slate | three-way split |
| Binary / PE analysis | forge | — | unique |
| Media / audio | flux | — | unique |
| Network transfer | vortex | — | unique |
| Archives | chronos (pack) | arc | arc is the real one |

---

## 3. The combination: 22 apps → 13 modules

Grouped by **what the user is trying to do**, not by how the code was written. Each module keeps a recognisable name so the identity of your work survives.

| # | Module | Job | Built from | Ops |
|---|---|---|---|---|
| 1 | **Aegis** | Encrypt, shred, scan for threats, audit persistence | aegis + vault | 6 |
| 2 | **Forge** | PE headers, sections, entropy, imports, hex, patching | forge + aegis audit | 6 |
| 3 | **Halo** | ISO authoring, extraction, sector conversion, CUE, cover art | halo + optic | 7 |
| 4 | **Iris** | Image inspect, convert, thumbnails, transform, batch, `.ico` | iris + prism | 8 |
| 5 | **Arc** | Archives, split/join, transfer planning, torrent, probe | arc + vortex | 10 |
| 6 | **Sift** | Strings, regex search, batch replace, XOR, text stats, EOL | sift + slate | 9 |
| 7 | **Glyph** | PDF generate/extract, Markdown → HTML, readability | glyph | 7 |
| 8 | **Scout** | Catalogue, duplicates, treemap, inventory, diff | scout + strata | 6 |
| 9 | **Morph** | Pattern rename with preview + undo journal | morph + strata | 2 rich |
| 10 | **Pulse** | Hardware profile, processes, drives, hashes, debloat | pulse + beacon | 7 |
| 11 | **Pager** | Window list, snap, pin, rollup, alpha, workspace switch | pager + dock | 9 |
| 12 | **Revive** | Carve, recover, snapshot pack/verify, disk geometry | revive + chronos | 7 |
| 13 | **Flux** | Audio inspect, synth, ID3 tags, gain | flux | 4 |

**~88 operations, one binary, one interface.** Nothing is deleted — every capability in both archives has a home.

Two judgement calls I want to flag:

- **Pager is the odd one out.** Every other module is a file-in / result-out tool. Pager manipulates other applications' windows, which needs a resident background process and per-platform window APIs (Win32 on Windows, Accessibility API on macOS, X11/Wayland on Linux). I recommend it ships as a module *plus* a small tray agent, and that Windows is its only fully supported platform in v1.
- **Vortex goes inside Arc.** Its "chunk plan", torrent parsing and TCP probe are transfer-adjacent. Its `simulate` command is a fake and should either become a real multi-connection downloader or be cut.

---

## 4. Repository decision: one repo

**Create one repository: `savehorse`** (private, or public — your call, but decide before I push).

Reasons against splitting into 13 repos: they share one core library, one design system, one CI matrix and one release cadence. Thirteen repos would mean thirteen places to bump the shared crate and thirteen CIs to keep green.

```
savehorse/
├── Cargo.toml                     workspace
├── rust-toolchain.toml
├── crates/
│   ├── savehorse-core/              shared: errors, safe paths, walker, hashing,
│   │                              progress events, JSON envelope, dry-run guard
│   ├── savehorse-aegis/             one crate per module
│   ├── savehorse-forge/
│   ├── … (13 module crates)
│   └── savehorse-registry/          the op manifest — single source of truth
├── apps/
│   ├── savehorse-cli/               binary `savehorse <module> <op>`
│   ├── savehorse-desktop/           Tauri 2 shell + React UI
│   └── savehorse-agent/             tray agent for Pager (optional, Phase 6)
├── docs/
│   ├── PLAN.md                    this document
│   ├── CAPABILITY-MATRIX.md       every op, old → new
│   └── decisions/                 architecture decision records
├── .github/workflows/             ci.yml + release.yml (win/mac/linux)
└── README.md
```

The only case for a second repo is `savehorse-agent`, and only if Pager grows into a standalone product. Not now.

### The one engineering idea that makes this work

**A single op manifest in Rust.** Every operation declares itself once — name, module, parameters with types, defaults, validation, danger level, and the CLI path:

```rust
Operation {
    id: "aegis.encrypt",
    params: [ Input(file), Output(file), Passphrase(secret), Keyfile(optional) ],
    danger: Danger::None,
    dry_run: false,
}
```

From that one declaration you get, automatically: the CLI parser, `--help`, the GUI form with correct controls and validation, the JSON schema, the API surface for recipes, and the test scaffold. This is what stops 88 operations from becoming 88 hand-written forms — and it is the difference between a tidy refactor and a maintainable product.

---

## 5. The GUI: "Instrument Grade"

The current GUIs are hand-painted Win32 windows. Rebuilding that pattern 13 more times would be a mistake. Replace the shell, keep the logic.

### Stack

| Choice | Decision | Why |
|---|---|---|
| Shell | **Tauri 2** | Keeps your Rust logic as library crates. Cross-platform in one codebase. Installers ~6 MB instead of ~120 MB for Electron. |
| UI | **React 19 + TypeScript + Vite + Tailwind v4** | Fastest path to a genuinely distinctive interface; huge ecosystem for hex viewers, treemaps, PDF preview. |
| Tokens | Custom design-token layer, not a component kit | Uniqueness has to come from the design system, not from a library's default look. |
| Rejected | egui / iced / slint | Pure-Rust and single-binary, but styling is constrained and the "unique" brief becomes hard. Viable fallback if you want zero WebView dependency. |
| Rejected | Modernised Win32 | Would keep 15,000 lines of duplicated FFI and Windows-only shipping. |

### Design language

- **Dark-first, two themes:** Graphite (default) and Paper (light). Neutral chrome, so the content carries the colour.
- **One accent hue per module**, from a fixed 13-step ramp. Aegis is amber, Halo is violet, Revive is green. You always know where you are without reading a title.
- **Type:** Geist Sans for interface, Geist Mono for all data — hex, hashes, paths, JSON. Monospace is not decoration here; it is the content.
- **Layout:** 56 px module rail on the left, command bar on top, split canvas in the middle (operation form ↔ live result), job dock at the bottom.

```
┌──────┬─────────────────────────────────────────────────────────┐
│      │  ⌘K  Search operations…                    ⬤ 3 running  │
│  ▣   ├──────────────────────────┬──────────────────────────────┤
│  ▤   │  AEGIS · Encrypt         │  RESULT                      │
│  ◆   │  ─────────────────────   │  ┌────────────────────────┐  │
│  ◈   │  Input    [ report.pdf ] │  │ status   ENCRYPTED     │  │
│  ◉   │  Output   [ report.aeg  ]│  │ cipher   AES-256-GCM   │  │
│  ◐   │  Passphrase ●●●●●●●●●    │  │ in       2.4 MB        │  │
│  ▢   │  Keyfile  [ none      ]  │  │ out      2.4 MB        │  │
│  ◇   │                          │  └────────────────────────┘  │
│  ●   │  [ Preview ] [ Run ]     │  savehorse aegis encrypt  ⧉ │
│      ├──────────────────────────┴──────────────────────────────┤
│      │  JOBS   ✓ 14:22 aegis.encrypt  2.4MB  1.2s              │
│      │         ⟳ 14:23 revive.carve   scanning…  [cancel]      │
└──────┴─────────────────────────────────────────────────────────┘
```

### Six things that make it not-another-file-utility

1. **Command palette over all ~88 operations.** Type "shred", "iso", "carve" and you are there. No hunting through menus. This is the primary navigation, not a bonus.
2. **Drop anything on the canvas.** The file's magic bytes decide what you probably want: a `.bin` offers Forge hexdump, Halo sector convert and Revive carve; a `.pdf` offers Glyph extract. Three suggestions, one click.
3. **Recipes.** Chain operations into a saved pipeline — *catalogue → rename → archive → encrypt* — then re-run it on a folder, from the GUI or `savehorse recipe run`. This turns 22 tools into an automation tool.
4. **Every action shows its CLI equivalent**, copyable. Discoverability for the GUI, automation for the terminal, and it documents itself.
5. **Dry-run by default on anything destructive** — shred, byte patch, batch replace, rename. You see a diff or preview and commit explicitly. Given these tools touch files irreversibly, this is not optional polish.
6. **Schema-driven forms.** The UI is generated from the op manifest, so every operation gets correct controls, validation, help text and danger warnings without per-screen work.

---

## 6. Migration plan

Each phase ends with something runnable and a check you can verify.

| Phase | Work | Exit criterion |
|---|---|---|
| **P0** Discovery | This document, capability matrix, decision records | You approve |
| **P1** Foundation | Workspace, `savehorse-core`, op manifest, CLI skeleton with legacy aliases (`aegis …` still works). Port **Sift** and **Scout** first | `savehorse sift search`, `savehorse scout catalog` pass golden-file tests |
| **P2** Module port | Move all 22 cores into 13 crates. Strip Win32. Replace Gen 1 hand-rolled crypto/CRC/PRNG with real crates. Add tests per op | Every op has ≥1 test; `cargo test` green on win/mac/linux |
| **P3** Desktop shell | Tauri app, design system, palette, split view, job dock. Ship 4 modules fully, then the rest | App runs on all three OSes; 13 modules reachable |
| **P4** Differentiators | Recipes, drop-suggest, CLI echo, dry-run previews, history | A recipe can chain 3 modules end to end |
| **P5** Release | CI matrix, signed installers (`.msi`, `.dmg`, `.AppImage`/`.deb`), auto-update, docs site | v1.0.0 installable from a release page |
| **P6** Pager agent | Resident tray agent, global hotkeys, per-platform window APIs | Pager works on Windows; degrades gracefully elsewhere |

**Order matters.** P1 ports two *simple* modules first to prove the pattern, then P2 scales it. The test net gets built before the risky refactors, not after.

### First push (after you create the repo)

1. `PLAN.md`, `CAPABILITY-MATRIX.md`, decision records
2. Workspace skeleton + `savehorse-core` with tests
3. Op-manifest crate with the full 88-operation registry declared
4. Two modules ported and tested end to end (Sift, Scout)
5. CLI that builds and runs
6. CI workflow that actually fails when tests fail

That gives you a working spine to review before the remaining 11 modules move.

---

## 7. Risks, stated plainly

| Risk | Reality | Mitigation |
|---|---|---|
| **No tests, 29,630 lines** | Biggest risk. A port without a safety net silently changes behaviour. | Characterise each op's current CLI output as a golden file *before* refactoring. P1 proves the harness on two modules. |
| **Gen 1 crypto is unsound** | `aegis` uses an LCG PRNG for salts/keys and a home-made CRC32. | Never ship it. Port `vault`'s `aes-gcm` + `pbkdf2` implementation instead; retire the aegis container format or migrate it. |
| **Simulated features** | `vortex simulate` is fake. `flux` has no real transcoder. | Label honestly in the UI, or cut from v1. I will not present them as working. |
| **Pager platform scope** | Window APIs are not portable. | Windows-first in v1, documented as such. |
| **WebView2 dependency** | Windows 10/11 has it; some older/LTSC setups do not; Linux needs `webkit2gtk-4.1`. | Bundle the WebView2 bootstrapper in the Windows installer; document Linux packages. |
| **Licence** | PolyForm Noncommercial 1.0.0 blocks commercial use — including selling the app. | Decide now: keep it, or relicense to MIT/Apache-2.0 if you ever want distribution or paid tiers. |
| **Scope** | 13 modules × GUI + tests is months of work, not days. | Phase gates above. Nothing is "done" until its test runs. |

I can compile and test the portable crates in my environment (Rust toolchain installs fine), so P1 and P2 are verifiable before anything reaches the repo. The Tauri GUI and the Windows-only modules get verified in CI.

---

## 8. What I need from you

**Repository:** `drsamonline/savehorse` — created, public, and the spine is pushed.

Then confirm these four decisions, or override them:

| # | Decision | My recommendation |
|---|---|---|
| 1 | Repo shape | **One monorepo, `savehorse`** |
| 2 | GUI stack | **Tauri 2 + React + TS** (fallback: pure-Rust egui) |
| 3 | Platform | **Windows + macOS + Linux** (18 of 22 cores already portable) |
| 4 | Licence | Keep PolyForm NC for now — **change if you ever plan to sell** |

**The name: `savehorse`.** Chosen by you; the repository is
[drsamonline/savehorse](https://github.com/drsamonline/savehorse).

It is verified clear as a software name:

| Check | Result |
|---|---|
| crates.io | free |
| npm | free |
| GitHub repositories with that name | 1 — this one |
| Existing software product | none found |

The names that were considered and rejected, for the record:

| Candidate | Verdict |
|---|---|
| tessera | taken — your own repo `drsamonline/tessera` |
| spokeshave | software product exists at spokeshave.lotlsoft.com |
| handplane | software product exists — Handplane Baker (handplane3d.com) |
| toolwright | heavily taken: toolwright.com is *138 file tools*, plus .io, .co.uk, .studio, a GitHub org |
| cairn, kerf, adze, awl, auger, basalt, orrery, bezel | crate name taken or 500+ repos using it |

The metaphor survives the spelling: a shave horse is the bench a chairmaker
sits on, clamping the work with a foot pedal so both hands stay free for the
tool. One bench, thirteen tools, each keeping its own name.

Once the repo exists and these are settled, I push the first commit and we start on P1.
