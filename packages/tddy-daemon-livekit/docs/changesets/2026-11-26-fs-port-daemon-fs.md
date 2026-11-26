# 2026-11-26 — Disk I/O through the Fs port

**Type:** Architecture · PR [#1047](https://github.com/uppin/tddy-coder/pull/1047) (`#fs-port` 3/13)

the scratch-index seeding through the port: production code reaches disk only through `tddy-fs`, with `DiskFs` at composition
sites and `MemFs` in tests. `tests/fs_port.rs` carries the `direct_disk_io_in` guard plus
one behaviour test. Cross-package entry:
[`docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md](../../../../docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md).
