# 2026-11-28 — Agent launch goes through the Fs port

**Type:** Refactoring · PR [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)

Launches read and write through `tddy-fs` behind `LaunchSessions.fs` and threaded seams;
`DiskFs` stays at constructors and old-name wrappers. `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-agents-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-agents-fs.md).
