# 2026-11-27 — Session files and worktrees go through the Fs port

**Type:** Refactoring · **PR:** [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)
**Packages:** `tddy-fs`, `tddy-session-files`, `tddy-session-sync`, `tddy-subagent-worktree`,
`tddy-session-worktree`, `tddy-worktree-service`
**Base:** origin/master at `681d10612` (n1–n4 landed; this node rebased onto it clean)

The production disk I/O of the five session-files crates goes through the `Fs` port. Each crate's
disk-touching entry points take an injected handle (`&dyn Fs`, `&AsyncFs`, or a struct holding
`Arc<dyn Fs>`) and delegate to new `<fn>_on` seams, with `DiskFs` only at composition roots:
service constructors, request handlers, and backward-compat wrappers kept for callers outside the
migrated crates (`tddy-daemon-rpc`, `tddy-agent-launch`, `tddy-session-activity`,
`tddy-daemon/tests`, `tddy-session-lifecycle/tests`, `tddy-integration-tests`). Async call sites
go through `AsyncFs`. Git-subprocess-bound paths (the sync `Mirror`, conversation worktrees, index
scratching) inject the handle but bind `DiskFs` — a `MemFs` could never back the repositories git
itself reads and writes. Every crate carries `tests/fs_port.rs` with the `direct_disk_io_in`
guard over its own `src/`; `tddy-session-files` additionally carries the contract's `MemFs`
behaviour tests (uploads listing, empty-uploads listing).

The port grows additively (no existing signature or behaviour changes): `Fs::create_new` —
exclusive create, `AlreadyExists` when anything exists there — and `Fs::copy` (file copy,
`IsADirectory` on a directory source on both adapters), each with `DiskFs` + `MemFs`
implementations and conformance tests, plus an `AsyncFs::copy` facade. The attachment writers
need atomic check-and-create; the worktree scratch-index seeding needs a real copy; emulating
either would trade documented atomicity for a race. Detail: [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md).

Known gaps (all recorded here rather than silently left): `fast_forward_home_worktree` is pure
git with zero filesystem ops (no handle to inject); `test_util.rs` writes through `DiskFs` by
design (ungated test helper for downstream integration suites); per-crate/module metrics labels
on Fs call sites were requested but no such requirement exists in the changeset, PRD, or ledger —
left as an open question for the metrics owner, not this node. Test-only crates
(testing-commons, e2e, vm-testkit, test-hosts) keep real disk.
