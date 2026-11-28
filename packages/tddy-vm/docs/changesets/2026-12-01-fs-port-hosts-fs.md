# 2026-12-01 — Hosts, VMs, git and projects go through the Fs port

**Type:** Refactoring · PR [#1056](https://github.com/uppin/tddy-coder/pull/1056) (`#fs-port` 12/13)

This crate's production disk I/O goes through `tddy-fs` behind injected handles and
`<fn>_on` seams; `DiskFs` stays at composition roots and old-name wrappers.
`tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-12-01-fs-port-hosts-fs.md`](../../../../docs/dev/changesets/2026-12-01-fs-port-hosts-fs.md).
