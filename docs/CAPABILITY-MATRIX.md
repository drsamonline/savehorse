# Capability matrix

Every operation from both archives, where it lives now, and whether it runs.

Source archives: `releases.7z` (generation 1, eleven apps) and `releases.7z.001`
(generation 2, eleven apps). Twenty-two applications, 29,630 lines of Rust,
folded into thirteen modules.

Status values: **ready** — ported, tested, callable today. **planned** — declared
in the manifest, code not yet moved.

---

## aegis — Vault & Security

From `aegis` (gen 1) and `vault` (gen 2).

| Operation | What it does | Status |
|---|---|---|
| `aegis.encrypt` | AES-256-GCM encrypt with passphrase or keyfile | **ready** |
| `aegis.decrypt` | Decrypt a container | **ready** |
| `aegis.shred` | DoD 5220.22-M three-pass overwrite and remove | **ready** |
| `aegis.scan` | Signature and heuristic threat scan | **ready** |
| `aegis.persistence` | Audit startup and persistence vectors | planned |
| `aegis.harden` | Audit DEP, ASLR and signing on an executable | **ready** |

**Porting note — autostart was made portable.** The original read the Windows
registry, which needs a platform crate and works on one operating system. Every
desktop has a file-based autostart directory — `.desktop` entries on Linux,
launch agents on macOS, Startup folders on Windows — and reading those needs no
dependency. The report names the locations it checked and says plainly that the
registry is not among them.

**Porting note — the cryptography was replaced, not carried.** Gen 1 `aegis`
derived keys from a hand-written linear congruential generator seeded with the
file size, and used a home-made CRC-32 as its integrity check. Neither is a
security primitive. The encryption here is `vault`'s: AES-256-GCM, PBKDF2-
HMAC-SHA256 over 100,000 rounds, a random salt and nonce from the OS entropy
source.

The container keeps vault's `VAULT\x01` header byte for byte, so files the old
`vault` wrote still open. Containers written by the old `aegis` (magic
`AEGISENC`) do not: they were encrypted with the broken generator, and the
honest migration is to decrypt them once with the old binary and re-encrypt.

Shredding's third pass now draws from the OS entropy source rather than a
sequence derived from the file size, which anyone who knows the size could
reproduce. `aegis.scan` runs its heuristics always, and matches a signature
database only when one is supplied — with none, it says so rather than
implying it checked.

## forge — Binary Lab

From `forge` (gen 1). No gen 2 equivalent; this module is the only home for
these operations.

**Porting note.** `patch` can preview, refuses an offset that would run past
the end of the file, and only then writes. `inspect` no longer reads a fixed
72 bytes when the optional header claims to be empty, which could quietly read
into the section table. `imports` is a heuristic — it scans for printable
strings ending in `.dll` — and is described that way rather than as a real
import-table walk.

| Operation | What it does | Status |
|---|---|---|
| `forge.inspect` | DOS, COFF and optional PE headers | **ready** |
| `forge.sections` | Section headers with Shannon entropy | **ready** |
| `forge.imports` | Imported dynamic libraries | **ready** |
| `forge.hexdump` | Hex and ASCII view | **ready** |
| `forge.search` | Find a hex pattern | **ready** |
| `forge.patch` | Overwrite bytes at an offset | **ready** |

## halo — Disc Studio

From `halo` (gen 2) and `optic` (gen 1). `optic` is the superset: it also wrote
CUE sheets and cover artwork.

**Porting note — three defects fixed, not carried forward.** The gen 2 writer
produced images that looked right and were not:

1. The volume descriptor claimed a 10-byte path table at sector 18, which was
   where the root directory actually lived. No path table was ever written. A
   real L and M path table now exist and the descriptor points at them.
2. `create` walked one level and packed only top-level files, silently leaving
   out every subdirectory. It now refuses with an error rather than producing
   an image that looks complete.
3. File records that did not fit in the single root-directory sector were
   dropped with a `break`. It now errors, naming the limit.

Two source files that sanitise to the same ISO name are also disambiguated now
instead of colliding.

| Operation | What it does | Status |
|---|---|---|
| `halo.create` | Build an ISO 9660 image | **ready** |
| `halo.list` | List a disc image | **ready** |
| `halo.extract` | Extract every file | **ready** |
| `halo.verify` | Volume descriptors and checksums | **ready** |
| `halo.convert` | Raw 2352-byte sectors to 2048-byte ISO | **ready** |
| `halo.cue` | Write a CUE sheet | **ready** |
| `halo.cover` | Disc label and case artwork | **ready** |

## iris — Image Studio

From `iris` (gen 2) and `prism` (gen 1). `prism` contributes `.ico` generation
and contact sheets.

| Operation | What it does | Status |
|---|---|---|
| `iris.info` | Dimensions, format, bit depth, aspect | **ready** |
| `iris.convert` | Convert between formats | **ready** |
| `iris.thumb` | Aspect-preserving downscale | **ready** |
| `iris.transform` | Rotate and flip | **ready** |
| `iris.stats` | Colour and luminance statistics | **ready** |
| `iris.batch` | One operation across a folder | **ready** |
| `iris.ico` | Multi-resolution Windows icon | **ready** |
| `iris.sheet` | Contact sheet | **ready** |

## arc — Archives

From `arc` (gen 2) and `vortex` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `arc.create` | Pack files | **ready** |
| `arc.extract` | Unpack | **ready** |
| `arc.list` | List contents | **ready** |
| `arc.test` | Verify integrity | **ready** |
| `arc.split` | Split into volumes | **ready** |
| `arc.join` | Reassemble volumes | **ready** |
| `arc.chunk` | Plan a multi-segment download | **ready** |
| `arc.torrent` | Read `.torrent` metadata | **ready** |
| `arc.probe` | Probe and time a TCP endpoint | **ready** |
| `arc.upnp` | UPnP port-mapping payloads | **ready** |

**Porting note.** `vortex`'s transfer command was a simulation, not a
downloader. It is declared here but should become real or be cut; it will not
be shipped as a pretend feature.

**Archive note.** Extraction passes every entry through `enclosed_name`, so an
archive cannot write outside the destination directory — the original had this
right and it is kept.

## sift — Text & Search

From `sift` (gen 2) and `slate` (gen 2). `glyph`'s code-metrics and search
operations also land here.

| Operation | What it does | Status |
|---|---|---|
| `sift.strings` | Extract printable ASCII and UTF-16 strings | **ready** |
| `sift.search` | Literal or regex search with line numbers | **ready** |
| `sift.replace` | Batch replace with preview and backup | **ready** |
| `sift.xor` | Recover XOR-obfuscated text | **ready** |
| `sift.view` | Read a text file with line numbers | **ready** |
| `sift.eol` | Normalise line endings | **ready** |
| `sift.clean` | Strip trailing whitespace | **ready** |
| `sift.stats` | Lines, words, characters, encoding | **ready** |
| `sift.convert` | Convert between encodings | **ready** |

**Text note.** The `view`, `eol`, `clean`, `stats` and `convert` operations came
from `slate` (gen 2) and live in the same crate, because reading and changing
text is one job. Two behaviours changed on purpose: `convert` requires an
explicit output rather than overwriting the source when none is given, and
`view` ends lines with `\n` instead of `\r\n`.

**Porting note.** The original searched inside `.zip` archives. That needs the
`zip` crate and is deferred, not dropped — it will return as `sift.search`
gains an `--archives` flag.

## glyph — Documents

From `glyph` (gen 1).

**Note.** `glyph.search` has been **removed from the manifest**. It was a
substring scan over a single file, which `sift.search` already does with line
numbers, regex and glob filters. A manifest that lists an operation which will
never exist is worse than one that does not: the count is now 87, not 88.

| Operation | What it does | Status |
|---|---|---|
| `glyph.pdf` | Generate a PDF from text | **ready** |
| `glyph.extract` | Pull text out of a PDF | **ready** |
| `glyph.markdown` | Markdown to styled HTML | **ready** |
| `glyph.encoding` | Detect encoding and line endings | **ready** |
| `glyph.readability` | Word count, reading time, readability | **ready** |
| `glyph.metrics` | Code lines, comments, blanks | **ready** |
| ~~`glyph.search`~~ | Search a document | removed — `sift.search` does it |

## scout — Filesystem

From `scout` (gen 2) and `strata` (gen 1). `strata` contributes the treemap,
inventory and byte-diff.

| Operation | What it does | Status |
|---|---|---|
| `scout.catalog` | Catalogue to table, JSON, CSV, Markdown or HTML | **ready** |
| `scout.duplicates` | Find byte-identical files by SHA-256 | **ready** |
| `scout.clean` | Remove duplicates, preview first | **ready** |
| `scout.treemap` | Space usage treemap | **ready** |
| `scout.inventory` | Depth, capacity, largest files | **ready** |
| `scout.diff` | Byte-by-byte comparison | **ready** |

**Porting note.** Batch processing no longer swallows save errors. The original
wrote `let _ = img.save(...)` and counted the file as processed regardless, so a
run could report success while writing nothing; failures are now collected and
returned.

**Porting note.** The original computed a full SHA-256 of every file. The port
groups by size first and only hashes where a collision is possible, which is
what keeps a scan of a large tree affordable.

## morph — Rename Engine

From `morph` (gen 2) and `strata`'s rename (gen 1). `morph` has the undo
journal, so it wins.

**Porting note.** The original wrote the undo journal *after* renaming and
ignored the result. A crash halfway through left files renamed with no record
of how to put them back, and a journal that failed to write went unnoticed.
The journal is now written before anything moves, and failing to write it
stops the whole run. A real rename always leaves a journal, defaulting to
`.savehorse-rename-<time>.json` if none is named.

| Operation | What it does | Status |
|---|---|---|
| `morph.rename` | Pattern rename with collision safety | **ready** |
| `morph.undo` | Roll back the last transaction | **ready** |

## pulse — System

From `pulse` (gen 2) and `beacon` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `pulse.hardware` | CPU, memory, architecture | planned |
| `pulse.procs` | Processes, threads, priority | planned |
| `pulse.drives` | Drives, filesystems, free space | planned |
| `pulse.hash` | CRC32 and SHA-256 | **ready** |
| `pulse.temp` | Reclaimable temporary storage | **ready** |
| `pulse.services` | Background services audit | planned |
| `pulse.priority` | Change process priority | planned |

## pager — Window Manager

From `pager` (gen 2) and `dock` (gen 1).

**Porting note — this is the one module that cannot be portable.** Enumerating
and moving other applications' windows is a different API on every desktop, so
each platform gets its own backend: Win32 on Windows, `wmctrl` on Linux,
`osascript` on macOS. What each can do:

| | Windows | Linux | macOS |
|---|---|---|---|
| list, switch, snap, move, pin | yes | yes (needs `wmctrl`) | list and switch only |
| rollup, alpha | yes | not supported | not supported |

Anything a platform cannot do says so and names the platform. Nothing is
simulated. `calc` is pure arithmetic and works everywhere, which is why it
exists: you can work out where a window would land before letting anything move
it.

| Operation | What it does | Status |
|---|---|---|
| `pager.list` | Visible top-level windows | planned |
| `pager.switch` | Switch virtual desktop | planned |
| `pager.rollup` | Shade to title bar | planned |
| `pager.pin` | Keep above others | planned |
| `pager.alpha` | Window transparency | planned |
| `pager.snap` | Snap to a grid region | planned |
| `pager.move` | Move to a position or screen | planned |
| `pager.launch` | Start an application detached | planned |
| `pager.calc` | Compute grid snap bounds | planned |

**Porting note.** `report` was the same call as `scan` with different flags in
the original. Here it writes a real report — per-format totals and a plain
assessment — optionally to a file.

**Porting note.** This is the one module that cannot be a pure
file-in/result-out tool: it manipulates other applications' windows, which needs
a resident agent and a different implementation per platform (Win32, macOS
Accessibility, X11/Wayland). Windows is the only fully supported platform in
v1.

## revive — Recovery

From `revive` (gen 2) and `chronos` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `revive.scan` | Scan an image for recoverable files | **ready** |
| `revive.carve` | Carve files out by signature | **ready** |
| `revive.report` | Summarise a carving run | **ready** |
| `revive.pack` | Pack a tree into a snapshot | **ready** |
| `revive.unpack` | Restore a snapshot | **ready** |
| `revive.verify` | Check a snapshot's checksums | **ready** |
| `revive.geometry` | Partition headers and entropy | **ready** |

## flux — Audio

From `flux` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `flux.inspect` | Format, sample rate, channels, ID3 | planned |
| `flux.synth` | Synthesise a PCM WAV tone | planned |
| `flux.tag` | Write ID3 metadata | planned |
| `flux.gain` | Scale PCM volume | planned |

**Porting note.** `flux` could synthesise a tone and scale PCM, but there is no
real transcoder behind the media-engine description. It will be documented as
what it is.

---

## Totals

| | |
|---|---|
| Original applications | 22 |
| Modules | 13 |
| Operations declared | 87 |
| Operations ready | 86 |
| Operations planned | 1 |

The manifest in `crates/savehorse-registry/src/lib.rs` is the source of truth;
a unit test asserts the count so this table cannot drift from the code.
