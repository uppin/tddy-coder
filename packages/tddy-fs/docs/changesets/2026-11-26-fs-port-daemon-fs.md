# 2026-11-26 — Additive port surface for the daemon migration

**Type:** Architecture · PR [#1047](https://github.com/uppin/tddy-coder/pull/1047) (`#fs-port` 3/13)

The port grows additively for the daemon-crate migration (no existing signature or behaviour
changes): `hard_link`, `create_dir_all_with_mode`, `remove_dir`, `metadata_follow`,
`FsMetadata::{uid, dev, ino}`, `FsKind::Socket`, implemented on both adapters with
dual-adapter conformance tests. Detail: [the filesystem port](../filesystem-port.md).
Cross-package entry:
[`docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md](../../../../docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md).
