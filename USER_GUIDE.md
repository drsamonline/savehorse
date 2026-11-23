# savehorse user guide

savehorse is thirteen small tools in one program. You run it from a terminal, or
from the desktop app if you prefer a window. Every operation in the app has a
command line underneath it, and the app shows you what that command is, so you
can move between the two without relearning anything.

This guide is for using it. If you want to build it from source or understand
how it is put together, the [README](README.md) covers that.

---

## Before you start

You need a build, not a checkout. Either download a release from the
[releases page](https://github.com/drsamonline/savehorse/releases), or build it:

```sh
git clone https://github.com/drsamonline/savehorse
cd savehorse
cargo build --release -p savehorse-cli
```

The binary lands in `target/release/savehorse`. Copy it somewhere on your PATH,
or call it by path. There is nothing to install and no service to start.

To check it works:

```sh
savehorse --version
savehorse modules
```

`savehorse modules` prints all thirteen modules and how many of their operations
are ready. Operations that are not ported yet say so when you try to run them,
and they say it by name rather than failing somewhere confusing.

---

## The shape of a command

```
savehorse <module> <operation> [arguments] [flags]
```

Two examples:

```sh
savehorse sift search "TODO" ./src
savehorse iris convert photo.png --out photo.jpg
```

Arguments are positional and follow the order shown in the help. Flags start
with `--` and take a value as either `--out file.zip` or `--out=file.zip`. A
switch that has no value is written on its own, like `--dry-run`.

To see what a module can do:

```sh
savehorse sift --help
```

To see everything at once:

```sh
savehorse ops
```

---

## Who should use which module

The thirteen modules came from twenty-two separate programs. This is the short
version of who reaches for what.

| If you are | Start with | Because |
|---|---|---|
| Anyone who has lost files | `revive` | Carves images, PDFs, ZIPs and GIFs back out of a disk image or a corrupted file, and can pack and verify a directory snapshot. |
| A photographer or designer | `iris` | Batch converts a folder, makes contact sheets to review a shoot, and builds multi-size `.ico` files from one image. |
| A developer working in a codebase | `sift` | Search with regex across a tree, replace across files with a preview first, and read a file with line numbers. |
| Someone reading or writing documents | `glyph` | Generates a PDF from text, pulls the text back out of one, renders Markdown, and measures readability and code density. |
| A sysadmin or support technician | `pulse`, `halo` | Hash a file to prove two copies match, audit temporary storage, and build or verify an ISO for a machine you have to hand over. |
| Anyone cleaning a full disk | `scout` | Shows what is taking the space, finds byte-identical duplicates, and deletes them only when you say `--commit`. |
| Someone handling private files | `aegis` | Encrypts with a passphrase, shreds a file in three passes, and audits autostart entries and executable hardening. |
| A security student, or anyone doing forensics | `forge`, `sift` | Reads PE headers and sections, dumps and searches hex, patches bytes, and recovers XOR-obfuscated strings. |
| Someone who just wants files organised | `morph`, `arc` | Renames a batch by pattern, case or number with an undo journal, and packs or splits archives. |
| A musician or audio tinkerer | `flux` | Reads WAV and ID3 metadata, writes ID3v1 tags, generates test tones and applies gain. |
| Anyone on Windows who wants their desktop tidier | `pager` (not ready) | Window snapping and pinning. This module is still being ported. |

If none of those is you, run `savehorse ops` and read the one-line summary on
each operation. The summaries are accurate. They are also what the app shows
you.

---

## Five jobs people actually have

These are worked through, start to finish.

### Find every use of a word across a project

```sh
savehorse sift search "old_api_name" ./src --regex
```

You get `path:line: text` for each hit and a count at the end. Add
`--ignore-case` if the case does not matter, and `--glob "*.rs"` to look only at
Rust files.

### Replace it, without breaking anything

Preview first. Always, on the first run:

```sh
savehorse sift replace "old_api_name" "new_api_name" ./src --dry-run
```

That prints one line per file that would change and touches nothing. When the
list looks right, run it again without `--dry-run`, and add `--backup` so each
file keeps a `.bak` beside it:

```sh
savehorse sift replace "old_api_name" "new_api_name" ./src --backup
```

### Find out what is filling a disk

```sh
savehorse scout treemap ~/Downloads --out treemap.svg
savehorse scout inventory ~/Downloads
```

The treemap is an SVG you can open in a browser. The inventory gives you counts,
total size, depth and the single largest file. Neither deletes anything.

Then, for duplicates:

```sh
savehorse scout duplicates ~/Downloads
```

You get clusters of byte-identical files with a SHA-256 for each. To remove the
redundant copies:

```sh
savehorse scout clean ~/Downloads --commit
```

Leave off `--commit` and it only tells you what it would remove.

### Put a file in a vault

```sh
savehorse aegis encrypt notes.txt --out notes.vault --passphrase
```

The passphrase is prompted for. If you would rather script it, `--passphrase
"..."` works, though it will end up in your shell history.

To get it back:

```sh
savehorse aegis decrypt notes.vault --out notes.txt --passphrase
```

A wrong passphrase produces one message and writes nothing. The format cannot
tell you whether the passphrase was wrong or the file was altered, and it does
not try.

### Recover files from a disk image

```sh
savehorse revive report disk.img
```

This scans without writing and tells you what it found. If the count looks
worth having:

```sh
savehorse revive carve disk.img --out recovered
```

It writes the recovered files into `recovered/` and leaves the image alone. Add
`--types png,jpg,pdf` to narrow the search.

---

## Module reference

Each entry lists what the module is for, the operations that are ready, and the
one thing most likely to catch you out.

### aegis — vault and security

`encrypt`, `decrypt`, `shred`, `scan`, `harden`, `persistence`

AES-256-GCM encryption with PBKDF2 key stretching, a three-pass shredder, a
heuristic threat scan, a PE hardening audit and an autostart audit.

The thing to know: `shred` cannot be undone and does not promise to defeat
flash wear levelling or a copy-on-write filesystem. It overwrites, truncates,
renames and unlinks. On an SSD with wear levelling, a block may survive
elsewhere; that is a property of the drive, not of the tool. For files that
really matter, encrypt the whole volume instead.

`aegis scan` runs its heuristics always and matches a signature database only if
you give it one with `--signatures file.json`. With none, it says
`0 signature(s) loaded` so a clean result is not mistaken for a thorough one.

### forge — the binary lab

`inspect`, `sections`, `imports`, `hexdump`, `search`, `patch`

Reads PE headers, the section table with Shannon entropy, the DLL names it can
find, hex dumps, hex patterns, and patches bytes.

`patch` previews with `--dry-run`, refuses an offset that would run past the end
of the file, and only writes when you mean it. `--backup` keeps a `.bak`.
`imports` is a heuristic: it scans for printable strings ending in `.dll`, and
that is all it claims.

### halo — disc images

`create`, `list`, `extract`, `verify`, `convert`, `cue`, `cover`

Builds ISO 9660 images, reads and extracts them, converts raw 2352-byte sectors
to 2048, writes CUE sheets and renders jewel-case artwork as SVG.

`create` reads one directory level. If the source contains a subdirectory it
refuses rather than silently dropping it, because an image that looks complete
and is not is worse than an error.

### iris — images

`info`, `convert`, `thumb`, `transform`, `stats`, `batch`, `ico`, `sheet`

Inspects, converts, thumbnails, rotates and flips, reports channel statistics,
processes a folder in one go, builds multi-resolution `.ico` files and contact
sheets.

`batch` reports failures separately from successes. If a file cannot be decoded
it lands in the failed list and the run carries on.

### arc — archives and transfer

`create`, `extract`, `list`, `test`, `split`, `join`, `chunk`, `torrent`,
`probe`, `upnp`

ZIP creation and extraction with path-traversal protection, volume splitting and
joining, byte-range planning, `.torrent` metadata, a TCP probe, and UPnP
payloads.

`upnp` builds the two payloads a port mapping needs and sends nothing. `probe`
opens a connection and times it, and tells you nothing about what is behind it.

### sift — text and search

`strings`, `search`, `replace`, `xor`, `view`, `eol`, `clean`, `stats`,
`convert`

Extracts printable strings from a binary, searches, replaces, recovers
XOR-obfuscated text, reads with line numbers, normalises line endings, strips
trailing whitespace, counts, and converts encoding.

`eol`, `clean` and `replace` all change files. `eol` and `clean` only write when
you pass `--in-place`. `convert` requires `--out` and will not overwrite its
input.

### glyph — documents

`pdf`, `extract`, `markdown`, `encoding`, `readability`, `metrics`

Generates a PDF from a text file, pulls text back out of a simple PDF, renders
Markdown to HTML, detects encoding and line endings, and measures readability
and code density.

The PDF writer produces a single page of plain text with no fonts embedded
beyond the standard set. It is for notes and reports, not for typesetting.

### scout — filesystem

`catalog`, `duplicates`, `clean`, `treemap`, `inventory`, `diff`

Catalogue a tree to table, JSON, CSV, Markdown or HTML, find byte-identical
duplicates, remove them, draw a treemap, count what is there, and compare two
files byte for byte.

`clean` deletes only with `--commit`. Everything else in this module is read-only.

### morph — rename engine

`rename`, `undo`

Renames by regex, case, prefix, suffix, numbering or extension, with collision
detection and an undo journal.

The journal is written before anything moves. If it cannot be written, the run
stops rather than leaving you with renames you cannot reverse. A real rename
always leaves one, defaulting to `.savehorse-rename-<time>.json` in the working
directory.

```sh
savehorse morph rename ./photos --case title --number --journal undo.json
savehorse morph undo undo.json
```

### pulse — system

`hash`, `temp`, `hardware`, `procs`, `drives`, `services`

CRC-32 and SHA-256 of a file, an audit of temporary storage, the machine's
processor and memory, the busiest processes, mounted volumes and their free
space, and the services the platform knows about.

Everything here is read-only. `services` asks systemd, launchd or `sc` what is
installed and reports the command it used, so you can check the numbers
yourself. Nothing is started, stopped or enabled.

`priority` changes a process's scheduling class: idle, below-normal, normal,
above-normal, high or realtime. It previews by default, so nothing moves until
you add `--commit`.

```sh
savehorse pulse priority 4821 --level idle
savehorse pulse priority 4821 --level high --commit
```

Raising a process above normal needs root on Unix and administrator rights on
Windows. The refusal is reported as it happened rather than glossed over.

### revive — recovery

`scan`, `carve`, `report`, `pack`, `unpack`, `verify`, `geometry`

Carves PNG, JPEG, PDF, ZIP and GIF out of a blob by signature, packs a directory
into a `.chronos` snapshot, verifies it against CRC-32, restores it, and reads a
disk image's partition scheme and entropy.

The snapshot format is byte-for-byte the one the original `chronos` wrote, so
older archives still restore.

### flux — audio

`inspect`, `synth`, `tag`, `gain`

Reads WAV headers and ID3v1 tags, writes ID3v1 tags, generates a sine tone, and
scales samples with a clipping count.

ID3v1 is latin-1 and truncates at thirty characters. `tag` prints what was
actually stored, not what you asked for.

### pager — window manager

`list`, `switch`, `rollup`, `pin`, `alpha`, `snap`, `move`, `launch`, `calc`

Lists windows, brings one to the front, snaps it to a half or corner, moves it to
exact coordinates, pins it above others, changes its opacity, shades it to the
title bar, and starts a program.

This module is the one that cannot be the same everywhere. On Windows it is
Win32 throughout. On Linux it drives `wmctrl`, so you need that installed. On
macOS only listing and switching work, through `osascript`, and the terminal
needs Accessibility permission.

`calc` needs none of that: it is arithmetic. Work out where a window would land
before letting anything move it.

```sh
savehorse pager calc --position left --width 1920 --height 1080
savehorse pager list
savehorse pager snap 0x1A2B3C --position right
```

---

## Safety: what previews and what does not

Operations that destroy or rewrite data preview when you pass `--dry-run`:

| Operation | What `--dry-run` does |
|---|---|
| `aegis shred` | Reports what it would overwrite, then stops |
| `forge patch` | Shows the old and new bytes, writes nothing |
| `sift replace` | Lists each file that would change |
| `morph rename` | Lists every rename, moves nothing |
| `morph undo` | Lists the reversals, moves nothing |

`scout clean` uses `--commit` instead: without it, it is a preview.

`aegis shred` is the one operation with no recovery path. Read the dry run.

---

## Files savehorse writes

| File | Written by | Purpose |
|---|---|---|
| `*.vault` | `aegis encrypt` | The encrypted container |
| `*.bak` | `--backup` on `sift replace`, `forge patch` | The original, before it changed |
| `.savehorse-rename-<time>.json` | `morph rename` | Undo journal, consumed by a successful undo |
| `*.chronos` | `revive pack` | A directory snapshot |
| `*.iso`, `*.cue`, `*.svg` | `halo` | Images, CUE sheets, cover art |
| `*.ico`, `*.svg` | `iris` | Icons and contact sheets |

Nothing writes outside the path you give it. Archive extraction refuses entries
whose path would escape the destination, and snapshot restore filters the same
way.

---

## When something goes wrong

**`not ported yet`** — the operation exists in the manifest but the code has not
been moved across. Run `savehorse ops` to see which are ready.

**`--out is required`** — that operation will not guess a destination.

**`could not decrypt: wrong passphrase, wrong keyfile, or the file was altered`**
— all three produce the same message on purpose. An authenticated cipher cannot
distinguish them without telling an attacker something useful.

**`'...' already exists on disk`** from `morph rename` — a target name collides
with a file that is not part of the run. Nothing was renamed.

**A command seems to do nothing** — check for `--dry-run`. Several operations
default to preview when you ask for one.

---

## What is not finished

Every operation in the manifest is ported and tested.

`savehorse ops` is the authoritative list, and it is generated from the same
manifest the program runs from, so it cannot drift out of date.
