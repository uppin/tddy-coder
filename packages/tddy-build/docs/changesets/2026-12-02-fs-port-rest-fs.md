# 2026-12-02 — Every remaining production crate goes through the Fs port

**Type:** Refactoring · PR [#1057](https://github.com/uppin/tddy-coder/pull/1057) (`#fs-port` 13/13)

This crate's production disk I/O goes through `tddy-fs` behind injected handles and
`<fn>_on` seams; `DiskFs` stays at composition roots and old-name wrappers.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-12-02-fs-port-rest-fs.md`](../../../../docs/dev/changesets/2026-12-02-fs-port-rest-fs.md).
