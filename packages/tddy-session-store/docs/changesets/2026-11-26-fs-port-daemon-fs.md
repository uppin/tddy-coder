# 2026-11-26 — Skip sockets in the session listing

**Type:** Fix · PR [#1047](https://github.com/uppin/tddy-coder/pull/1047) (`#fs-port` 3/13)

The session listing skips the new `FsKind::Socket` the way it skips unreadable entries: a
socket in a session directory is an endpoint, not a manifest. Cross-package entry:
[`docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md](../../../docs/dev/changesets/2026-11-26-fs-port-daemon-fs.md).
