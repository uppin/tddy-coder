# 2026-11-28 — Coding subagent launcher goes through the Fs port

**Type:** Refactoring · PR [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)

Headless conversation dirs (claim, written-files record, close, sweep) read and write through
`tddy-fs` behind `<fn>_on` seams; old names delegate `DiskFs`. `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-agents-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-agents-fs.md).
