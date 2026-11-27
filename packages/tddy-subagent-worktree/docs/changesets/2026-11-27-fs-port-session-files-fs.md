# 2026-11-27 — Subagent worktrees go through the Fs port

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Scratch-index seeding, exclude-entry maintenance and cleanup go through `AsyncFs`, injected per
call with `DiskFs` bound at the public entry points (git runs against the real disk).
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
