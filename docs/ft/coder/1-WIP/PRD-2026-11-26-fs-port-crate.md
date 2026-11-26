# PRD: One filesystem port for all disk I/O (`tddy-fs`)

**Date:** 2026-11-26 · **Type:** Technical / architecture · **Streak:** `#fs-port` n1 of 13

## Affected features
None user-visible. Technical foundation for every crate that touches disk; see [`docs/dev/guides/testing.md`](../../../dev/guides/testing.md) (cheap in-memory seams).

## Summary
Code calls `std::fs` / `tokio::fs` directly (~2,000 call sites). Behaviour that depends on disk can be
tested only against a real temp directory, and the crash-safe "replace a file without truncating it"
invariant exists in four copies. This PRD introduces a single **filesystem port** — the `Fs` trait in a
new std-only leaf crate `tddy-fs` — with a real-disk adapter and an in-memory adapter, and makes the
atomic write one of the port's operations.

## Proposed changes
- New crate `tddy-fs`: `Fs` trait (read, read_to_string, write, write_atomic, write_atomic_with_mode,
  create_dir_all, remove_file, remove_dir_all, rename, metadata/exists, read_dir, set_permissions,
  open for append/read streams), `DiskFs`, `MemFs` (in-memory tree incl. modes and atomic semantics),
  `AsyncFs` facade behind a `tokio` feature.
- `tddy-session-store::atomic_file` becomes a re-export of `tddy-fs` (old path keeps working).
- `tddy-credentials` drops its private copy and writes its vault through the port.
- `tddy-session-store` reads and writes through an injected `Arc<dyn Fs>`.

## Staying the same
On-disk formats, paths and permissions; the atomic-write guarantees (swap file, fsync, rename, dir fsync, create_new swap, mode applied at creation).

## Acceptance criteria
- [ ] `tddy-fs` depends on `std` + `uuid` only (plus optional `tokio`).
- [ ] `DiskFs` and `MemFs` pass one shared conformance suite, including the atomic-write failure cases.
- [ ] `write_atomic*` exists in exactly one implementation; the credentials and agent-launch/task copies of the session-store helper are not reintroduced.
- [ ] `tddy-session-store` and `tddy-credentials` contain no direct `std::fs`/`tokio::fs` in production code.
- [ ] Existing session-store and credentials tests pass unchanged.
