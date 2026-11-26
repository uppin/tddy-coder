# 2026-11-26 — Disk I/O through the Fs port

No operator-visible change: technical foundation. The daemon crates read and write disk
through the `Fs` port — real disk in production, in-memory in tests. See [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md) and the cross-package entry
[`docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md`](../../../docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md)
(PR #1047).
