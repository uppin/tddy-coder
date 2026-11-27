# 2026-11-27 — Worktree service goes through the Fs port

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Readers, listings, size walks, stats cache, markers and removal checks run on the service-held
`Arc<dyn Fs>` (bound at construction) with `<fn>_on` seams; backward-compat wrappers keep old
signatures for daemon, agent-launch, activity and lifecycle callers. `tests/fs_port.rs` carries
the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
