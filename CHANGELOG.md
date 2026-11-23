# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing is released yet. This entry records what exists so far.

### Added

- Thirteen module crates, one operation manifest, one shared runner, a CLI and a
  Tauri desktop app.
- 73 of 87 operations ported and tested across Windows, macOS and Linux.
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

- `pulse` hardware, processes, drives, services and priority: five operations
  waiting on a cross-platform system API.
- `pager`: nine operations waiting on per-platform window APIs and a resident
  process.
