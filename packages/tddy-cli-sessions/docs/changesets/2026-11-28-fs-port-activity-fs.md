# 2026-11-28 — CLI sessions go through the Fs port

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

The tmux pane-env file is written through `Fs::create_new_with_mode` (exclusive, `0600` from
creation) behind `write_pane_env_file_on`; socket cleanup goes through the port. Ownership
hand-off calls `libc::chown` directly (no port operation). `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
