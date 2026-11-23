# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `scripts/check_docs.py`, run in CI: every `savehorse ...` command in the
  documentation is parsed against the manifest, so an example that does not
  parse cannot be merged.

### Fixed

- `--commit` was rejected on operations that did not declare it, including
  `pulse priority`, which the guide documents. It is a global switch beside
  `--dry-run` now.
- The documentation claimed the passphrase is prompted for. It is not, and
  never was; the examples show a value.
- The run button enabled while a required flag such as `--out` was still empty.
- Five colour pairs failed WCAG AA contrast; the landing page had no
  `:focus-visible` rule at all.

## [0.2.0] — 2026-11-23

Every operation in the manifest is now ported. Nothing is declared and waiting.

### Added

- `pulse priority`: set a process's scheduling class through
  `SetPriorityClass` on Windows and `setpriority` on Unix. It previews by
  default and only changes anything with `--commit`.
- A live terminal on the landing page, running real commands with the output
  the tool produces.

### Changed

- Releases publish on a tag rather than waiting as a draft, so a tag is visible
  as soon as its build passes.
- Release notes are taken from the changelog section for that version instead of
  the whole file.

### Fixed

- `savehorse ops | head` panicked on a broken pipe. SIGPIPE is restored, so a
  closed reader is a quiet exit.
- The "planned operation refuses" test and CI check named a specific operation
  and went stale each time one was ported. Both now ask the manifest.
- `pulse priority` reported a Windows priority class name on Unix. The label
  follows the platform now.

## [0.1.0] — 2026-11-23

First release. Thirteen modules, one command line, one desktop app.

### Added

- Thirteen module crates, one operation manifest, one shared runner, a CLI and a
  Tauri desktop app.
- 87 of 87 operations ported and tested across Windows, macOS and Linux.
- `savehorse ops`, generated from the manifest, which cannot drift from the code.
- A static interface demo built from the real app, with outputs captured from
  actual test runs.
- A release pipeline that builds installers for all three platforms and refuses
  to publish when the tag disagrees with the crate version.

### Changed from the twenty-two original applications

- Encryption moved from a hand-written generator to AES-256-GCM with PBKDF2.
  Containers written by the old `vault` still open; containers written by the old
  `aegis` do not, and cannot.
- The ISO writer now emits a real path table, refuses subdirectories rather than
  dropping them, and errors on overflow instead of truncating silently.
- Batch renaming writes its undo journal before moving anything, and stops if the
  journal cannot be written.
- Snapshot restore and archive extraction both filter paths that would escape
  their destination.
- `glyph.search` was removed. `sift.search` already does the same job better, and
  a manifest listing an operation that will never exist is worse than one that
  does not.

### Known gaps

- `pager` on macOS: listing and switching windows work through `osascript`;
  snapping, moving, pinning and opacity do not. Windows and Linux have the full
  set.
- The desktop app has not been run outside CI yet. It compiles and the interface
  is tested, but the first launch on a real machine is untested.
