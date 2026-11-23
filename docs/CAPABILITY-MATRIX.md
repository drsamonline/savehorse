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
| `aegis.encrypt` | AES-256-GCM encrypt with passphrase or keyfile | planned |
| `aegis.decrypt` | Decrypt a container | planned |
| `aegis.shred` | DoD 5220.22-M three-pass overwrite and remove | planned |
| `aegis.scan` | Signature and heuristic threat scan | planned |
| `aegis.persistence` | Audit startup and persistence vectors | planned |
| `aegis.harden` | Audit DEP, ASLR and signing on an executable | planned |

**Porting note.** Gen 1 `aegis` derived keys with a hand-written LCG PRNG and
its own CRC32. That is not cryptographic randomness and must not ship. The port
takes gen 2 `vault`'s implementation (`aes-gcm`, `pbkdf2`, `rand`, `sha2`) and
retires the old container format.

## forge — Binary Lab

From `forge` (gen 1). No gen 2 equivalent; this module is the only home for
these operations.

| Operation | What it does | Status |
|---|---|---|
| `forge.inspect` | DOS, COFF and optional PE headers | planned |
| `forge.sections` | Section headers with Shannon entropy | planned |
| `forge.imports` | Imported dynamic libraries | planned |
| `forge.hexdump` | Hex and ASCII view | planned |
| `forge.search` | Find a hex pattern | planned |
| `forge.patch` | Overwrite bytes at an offset | planned |

## halo — Disc Studio

From `halo` (gen 2) and `optic` (gen 1). `optic` is the superset: it also wrote
CUE sheets and cover artwork.

| Operation | What it does | Status |
|---|---|---|
| `halo.create` | Build an ISO 9660 image | planned |
| `halo.list` | List a disc image | planned |
| `halo.extract` | Extract every file | planned |
| `halo.verify` | Volume descriptors and checksums | planned |
| `halo.convert` | Raw 2352-byte sectors to 2048-byte ISO | planned |
| `halo.cue` | Write a CUE sheet | planned |
| `halo.cover` | Disc label and case artwork | planned |

## iris — Image Studio

From `iris` (gen 2) and `prism` (gen 1). `prism` contributes `.ico` generation
and contact sheets.

| Operation | What it does | Status |
|---|---|---|
| `iris.info` | Dimensions, format, bit depth, aspect | planned |
| `iris.convert` | Convert between formats | planned |
| `iris.thumb` | Aspect-preserving downscale | planned |
| `iris.transform` | Rotate and flip | planned |
| `iris.stats` | Colour and luminance statistics | planned |
| `iris.batch` | One operation across a folder | planned |
| `iris.ico` | Multi-resolution Windows icon | planned |
| `iris.sheet` | Contact sheet | planned |

## arc — Archives

From `arc` (gen 2) and `vortex` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `arc.create` | Pack files | planned |
| `arc.extract` | Unpack | planned |
| `arc.list` | List contents | planned |
| `arc.test` | Verify integrity | planned |
| `arc.split` | Split into volumes | planned |
| `arc.join` | Reassemble volumes | planned |
| `arc.chunk` | Plan a multi-segment download | planned |
| `arc.torrent` | Read `.torrent` metadata | planned |
| `arc.probe` | Probe and time a TCP endpoint | planned |
| `arc.upnp` | UPnP port-mapping payloads | planned |

**Porting note.** `vortex`'s transfer command was a simulation, not a
downloader. It is declared here but should become real or be cut; it will not
be shipped as a pretend feature.

## sift — Text & Search

From `sift` (gen 2) and `slate` (gen 2). `glyph`'s code-metrics and search
operations also land here.

| Operation | What it does | Status |
|---|---|---|
| `sift.strings` | Extract printable ASCII and UTF-16 strings | **ready** |
| `sift.search` | Literal or regex search with line numbers | **ready** |
| `sift.replace` | Batch replace with preview and backup | **ready** |
| `sift.xor` | Recover XOR-obfuscated text | **ready** |
| `sift.view` | Read a text file with line numbers | planned |
| `sift.eol` | Normalise line endings | planned |
| `sift.clean` | Strip trailing whitespace | planned |
| `sift.stats` | Lines, words, characters, encoding | planned |
| `sift.convert` | Convert between encodings | planned |

**Porting note.** The original searched inside `.zip` archives. That needs the
`zip` crate and is deferred, not dropped — it will return as `sift.search`
gains an `--archives` flag.

## glyph — Documents

From `glyph` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `glyph.pdf` | Generate a PDF from text | planned |
| `glyph.extract` | Pull text out of a PDF | planned |
| `glyph.markdown` | Markdown to styled HTML | planned |
| `glyph.encoding` | Detect encoding and line endings | planned |
| `glyph.readability` | Word count, reading time, readability | planned |
| `glyph.metrics` | Code lines, comments, blanks | planned |
| `glyph.search` | Search a document | planned |

## scout — Filesystem

From `scout` (gen 2) and `strata` (gen 1). `strata` contributes the treemap,
inventory and byte-diff.

| Operation | What it does | Status |
|---|---|---|
| `scout.catalog` | Catalogue to table, JSON, CSV, Markdown or HTML | **ready** |
| `scout.duplicates` | Find byte-identical files by SHA-256 | **ready** |
| `scout.clean` | Remove duplicates, preview first | **ready** |
| `scout.treemap` | Space usage treemap | planned |
| `scout.inventory` | Depth, capacity, largest files | planned |
| `scout.diff` | Byte-by-byte comparison | planned |

**Porting note.** The original computed a full SHA-256 of every file. The port
groups by size first and only hashes where a collision is possible, which is
what keeps a scan of a large tree affordable.

## morph — Rename Engine

From `morph` (gen 2) and `strata`'s rename (gen 1). `morph` has the undo
journal, so it wins.

| Operation | What it does | Status |
|---|---|---|
| `morph.rename` | Pattern rename with collision safety | planned |
| `morph.undo` | Roll back the last transaction | planned |

## pulse — System

From `pulse` (gen 2) and `beacon` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `pulse.hardware` | CPU, memory, architecture | planned |
| `pulse.procs` | Processes, threads, priority | planned |
| `pulse.drives` | Drives, filesystems, free space | planned |
| `pulse.hash` | CRC32 and SHA-256 | planned |
| `pulse.temp` | Reclaimable temporary storage | planned |
| `pulse.services` | Background services audit | planned |
| `pulse.priority` | Change process priority | planned |

## pager — Window Manager

From `pager` (gen 2) and `dock` (gen 1).

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

**Porting note.** This is the one module that cannot be a pure
file-in/result-out tool: it manipulates other applications' windows, which needs
a resident agent and a different implementation per platform (Win32, macOS
Accessibility, X11/Wayland). Windows is the only fully supported platform in
v1.

## revive — Recovery

From `revive` (gen 2) and `chronos` (gen 1).

| Operation | What it does | Status |
|---|---|---|
| `revive.scan` | Scan an image for recoverable files | planned |
| `revive.carve` | Carve files out by signature | planned |
| `revive.report` | Summarise a carving run | planned |
| `revive.pack` | Pack a tree into a snapshot | planned |
| `revive.unpack` | Restore a snapshot | planned |
| `revive.verify` | Check a snapshot's checksums | planned |
| `revive.geometry` | Partition headers and entropy | planned |

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
| Operations declared | 88 |
| Operations ready | 7 |
| Operations planned | 81 |

The manifest in `crates/shavehorse-registry/src/lib.rs` is the source of truth;
a unit test asserts the count so this table cannot drift from the code.
