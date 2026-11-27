# 2026-11-28 — Agent launch and backends go through the Fs port

**Type:** Refactoring · **PR:** [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)
**Packages:** `tddy-agent-launch`, `tddy-agent-backend`, `tddy-coding-subagent`, `tddy-agent-skills`, `tddy-coding-agent-specs`
**Base:** `feature/fs-port/activity-fs` at `5c5ebe551` (n6, itself on n5/master)

The production disk I/O of the five agent crates goes through the `Fs` port. Launches run
behind `LaunchSessions.fs`, backends behind held handles (`TurnFiles`, `UsageTap`,
`SpecHeadlessBackend::with_fs`, the conversation log appended per line), and free functions
delegate to `<fn>_on` seams, with `DiskFs` only at composition roots: service constructors,
daemon composition sites (`tddy-session-lifecycle`), and backward-compat wrappers kept for
callers outside the migrated crates. Every crate carries `tests/fs_port.rs` with the
`direct_disk_io_in` guard over its own `src/`. Detail: [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md).

No port surface was added on this node. Known gaps (all recorded here rather than silently
left): process spawning and socket binding stay on process/syscall APIs (the bound socket is
statted through the port); test files under `src/` are marked `#![cfg(test)]` instead of
extracted. Test-only crates keep real disk.
