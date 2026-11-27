# 2026-11-27 — Fs grows create_new and copy

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Two additive port methods for the session-files node: `Fs::create_new` (exclusive create,
`AlreadyExists` when anything exists there) and `Fs::copy` (file copy, `IsADirectory` on a
directory source on both adapters), each with `DiskFs` + `MemFs` implementations and conformance
tests, plus an `AsyncFs::copy` facade. No existing signature or behaviour changes.
Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
