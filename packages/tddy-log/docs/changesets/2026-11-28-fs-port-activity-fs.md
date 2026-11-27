# 2026-11-28 — Logging goes through the Fs port

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

Log files are appended per line through the installed `LOG_FS` handle (`DiskFs` until the
composition root calls `set_log_fs`) instead of held-open `File` handles; rotation, directory
creation and debug-output redirect go through the same handle. The `dup2` redirect calls
`libc::open` directly (the port has no fd). `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
