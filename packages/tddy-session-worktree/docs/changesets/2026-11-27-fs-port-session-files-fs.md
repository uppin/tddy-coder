# 2026-11-27 — Session worktree readers go through the Fs port

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

HEAD/ref resolution and chain-base canonicalization read through injected `&dyn Fs` with
`<fn>_on` seams; the old names delegate `DiskFs` for the `tddy-core` re-export and integration
tests. `tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
