# savehorse

One bench, thirteen tools.

savehorse is a single Rust workspace that replaces a portfolio of twenty-two
small Windows utilities with thirteen modules behind one command line — and,
next, one desktop application. Each module keeps the name it had on its own.

The name comes from the shave horse: the bench a chairmaker sits on, which
clamps the work with a foot pedal so both hands stay free for the tool. One
bench, thirteen tools, each keeping the name it had on its own.

## Status

Early. This is the spine, not the finished product.

| | |
|---|---|
| Modules | 13 |
| Operations declared | 88 |
| Operations ported | 42 — `sift` 9, `scout` 3, `glyph` 6, `halo` 5, `iris` 6, `arc` 6, `revive` 7 |
| GUI | Tauri 2 + React shell |
| Platforms | Windows, macOS, Linux |

Every operation is declared in the manifest whether or not it is ported yet, and
the CLI refuses a planned operation by name rather than failing somewhere
deeper. Nothing here pretends to work.

See [PLAN.md](PLAN.md) for the full consolidation plan, the module mapping and
the phased migration.

## Build

```sh
cargo build --workspace
cargo test --workspace
cargo run -- sift --help
```

Requires a stable Rust toolchain.

### The desktop app

```sh
cd apps/savehorse-desktop
npm install
npm run tauri dev      # the window, wired to the Rust core
npm run build          # type-check and build the interface on its own
```

The interface is a custom design system — tokens in
`src/styles/tokens.css`, layout in `src/styles/app.css`, no utility framework
and no webfont request, so it works offline like everything else here.

Inside the window, the operation list and every form come from the Rust
manifest over the bridge. Run the same code in a plain browser with
`npm run dev` and it says so: the module rail appears, the operations do not,
because there is no core to ask.

## Use

```
savehorse <module> <op> [arguments] [flags]

savehorse modules                 list the thirteen modules
savehorse ops [module]            list operations and their status
savehorse <module> --help         what one module can do
```

Examples:

```sh
# Find every mention of a word, as a literal or a regular expression
savehorse sift search "TODO" ./src --regex

# Pull printable strings out of a binary
savehorse sift strings ./app.exe --min-len 6

# Replace across a tree, previewing first
savehorse sift replace "old_name" "new_name" ./src --dry-run
savehorse sift replace "old_name" "new_name" ./src --backup

# Catalogue a folder as JSON, CSV, Markdown, HTML or a table
savehorse scout catalog ~/Pictures --format markdown --out catalog.md

# Find byte-identical files and reclaim the space
savehorse scout duplicates ~/Downloads
savehorse scout clean ~/Downloads            # preview
savehorse scout clean ~/Downloads --commit   # delete
```

### Flags

| Flag | Meaning |
|---|---|
| `--json` | machine-readable output on stdout |
| `--dry-run` | preview a destructive operation, change nothing |
| `--commit` | apply a destructive operation |
| `--backup` | keep a `.bak` copy before rewriting a file |
| `--regex` | treat the pattern as a regular expression |
| `--ignore-case` | case-insensitive matching |
| `--glob` | restrict to matching file names, e.g. `*.rs` |

Flags take a value as `--key value` or `--key=value`. The switches above never
take a value.

## The modules

| Module | Job | Replaces |
|---|---|---|
| `aegis` | Encrypt, shred, scan and audit | aegis + vault |
| `forge` | Inspect and patch executables | forge |
| `halo` | Author, read and convert disc images | halo + optic |
| `iris` | Inspect, convert and transform images | iris + prism |
| `arc` | Pack, split, join and transfer | arc + vortex |
| `sift` | Find, extract and replace across files | sift + slate |
| `glyph` | PDF, Markdown and readability | glyph |
| `scout` | Catalogue, deduplicate and compare | scout + strata |
| `morph` | Batch rename with preview and undo | morph + strata |
| `pulse` | Hardware, processes and maintenance | pulse + beacon |
| `pager` | Snap, pin and arrange windows | pager + dock |
| `revive` | Carve, recover and verify snapshots | revive + chronos |
| `flux` | Inspect, tag and shape audio | flux |

## Layout

```
crates/savehorse-core        errors, sizes, hashing, walking, path safety
crates/savehorse-registry    the operation manifest — declared once, read by all
crates/savehorse-sift        text and search
crates/savehorse-scout       filesystem catalogue and deduplication
crates/savehorse-runner      one implementation of every ported operation
apps/savehorse-cli           the `savehorse` binary
apps/savehorse-desktop       the Tauri window (React + TypeScript)
```

Each module crate holds logic only. It returns data or an error and never
prints. `savehorse-runner` turns a request into an outcome, and both the CLI
and the desktop app call it — so a behaviour change lands in both at once and
they cannot drift apart.

## Licence

PolyForm Noncommercial License 1.0.0. See [LICENSE](LICENSE).
