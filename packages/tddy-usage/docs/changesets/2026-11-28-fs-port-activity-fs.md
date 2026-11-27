# 2026-11-28 — Usage accounting goes through the Fs port

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

Usage context, conversation registry and JSONL folding read and write through `tddy-fs` behind
`<fn>_on` seams; old names delegate `DiskFs` for session-activity, agent-launch and sandbox-app
callers. Glob matching and the sqlite source stay on their crates (no port operation).
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
