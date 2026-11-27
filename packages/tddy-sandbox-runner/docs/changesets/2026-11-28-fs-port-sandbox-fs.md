# 2026-11-28 — Sandbox crates go through the Fs port

**Type:** Refactoring · PR [#1053](https://github.com/uppin/tddy-coder/pull/1053) (`#fs-port` 9/13)

This crate's production disk I/O goes through `tddy-fs` behind injected handles and
`<fn>_on` seams; `DiskFs` stays at composition roots and old-name wrappers.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-sandbox-fs.md`](../../../../docs/dev/changesets/2026-11-28-fs-port-sandbox-fs.md).
