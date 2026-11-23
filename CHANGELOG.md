# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] — 2026-11-23

First release. Thirteen modules, one command line, one desktop app.

### Added

- Thirteen module crates, one operation manifest, one shared runner, a CLI and a
  Tauri desktop app.
- 86 of 87 operations ported and tested across Windows, macOS and Linux.
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

- `pulse priority`: changing a process's priority needs a privileged call on
  every operating system, and a wrong implementation makes a machine unusable.
  One operation out of eighty-seven.
- `pager` on macOS: listing and switching windows work through `osascript`;
  snapping, moving, pinning and opacity do not. Windows and Linux have the full
  set.
- The desktop app has not been run outside CI yet. It compiles and the interface
  is tested, but the first launch on a real machine is untested.
