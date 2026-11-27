# 2026-11-26 — Disk I/O through the Fs port

**Type:** Architecture · PR [#1048](https://github.com/uppin/tddy-coder/pull/1048) (`#fs-port` 4/13)

channel manifests (byte-for-byte) and caller-capture overrides through the port: production code reaches disk only through `tddy-fs`, with `DiskFs` at composition
sites and `MemFs` in tests. `tests/fs_port.rs` carries the `direct_disk_io_in` guard plus
one behaviour test. Cross-package entry:
[`docs/dev/changesets/2026-11-26-fs-port-lifecycle-fs.md](../../../../docs/dev/changesets/2026-11-26-fs-port-lifecycle-fs.md).
