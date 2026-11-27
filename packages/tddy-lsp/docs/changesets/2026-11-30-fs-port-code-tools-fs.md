# 2026-11-30 — Code tools go through the Fs port

**Type:** Refactoring · PR [#1055](https://github.com/uppin/tddy-coder/pull/1055) (`#fs-port` 11/13)

This crate's production disk I/O goes through `tddy-fs` behind injected handles and
`<fn>_on` seams; `DiskFs` stays at composition roots and old-name wrappers.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-30-fs-port-code-tools-fs.md`](../../../../docs/dev/changesets/2026-11-30-fs-port-code-tools-fs.md).
