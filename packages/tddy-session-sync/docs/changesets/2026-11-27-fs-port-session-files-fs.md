# 2026-11-27 — Session mirror goes through the Fs port

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

The sync `Mirror` threads an injected `&dyn Fs` through open, read, apply, restore and the marker
and adoptability checks; `Syncer` holds it and binds `DiskFs` at construction (git runs against
the real disk, so `MemFs` could never back it). `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
