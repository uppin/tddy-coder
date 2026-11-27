# 2026-11-29 — Workflow, changeset and task crates go through the Fs port

**Type:** Refactoring · PR [#1054](https://github.com/uppin/tddy-coder/pull/1054) (`#fs-port` 10/13)

This crate's production disk I/O goes through `tddy-fs` behind injected handles and
`<fn>_on` seams; `DiskFs` stays at composition roots and old-name wrappers.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-29-fs-port-workflow-fs.md`](../../../../docs/dev/changesets/2026-11-29-fs-port-workflow-fs.md).
