# Changeset: `tddy-fs` — the filesystem port, with session-store and credentials on it

**Date**: 2026-11-26
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — new crate + two consumer crates; adapter seam; conformance semantics (atomicity, modes)
**PR**: [#1045](https://github.com/uppin/tddy-coder/pull/1045)
**PRD**: [PRD-2026-11-26-fs-port-crate.md](../../ft/coder/1-WIP/PRD-2026-11-26-fs-port-crate.md)

**Sequencing (hard gate):** do not start `/green` before `#target-reconnect` #1041 and #1042 have merged — they edit agent-launch, cli-sessions and session-lifecycle, which this streak migrates. Implementation runs as `#muse` sessions on `mantas-udoo-ros` (one session at a time, tight disk): keep every gate scoped with `./test -p <crate>` per crate, never a workspace run.

## Responsibility
Owns crate `packages/tddy-fs`: the `Fs` trait, `DiskFs`, `MemFs`, the `AsyncFs` facade (`tokio` feature),
`write_atomic` / `write_atomic_with_mode` / `write_atomic_labelled` as port operations and free functions,
and a public conformance-suite macro/fn every adapter runs. Migrates `tddy-session-store` and
`tddy-credentials` production code onto it.

## Boundaries
- Migrates no crate other than `tddy-session-store` and `tddy-credentials`; other `write_atomic` callers
  keep compiling through the session-store re-export until their node.
- Does not change any on-disk format or path.
- Does not remove `tddy-agent-launch::log_follower::write_atomically` or `tddy-task::capture_recording::write_atomically` (n7, n10).

## Placement
**Streak:** `#fs-port` — n1 of 13
**Unit:** stack `fs-port` — position 1 of 12 (**n1** · n3 · n4 · n5 · n6 · n7 · n8 · n9 · n10 · n11 · n12 · n13)
**Base:** `master`
**Why stacked:** every migration node (n3–n13) tests through this node's `MemFs` and compiles against `Fs`.

## Dependencies
None in the streak. Lands after `#target-reconnect` (#1041, #1042) — a scheduling fact, not a code edge.

## Draft PR contract
First push: `tddy-fs` crate with `Fs`, `DiskFs`, `MemFs`, `AsyncFs` signatures (bodies `// TODO(fs-crate): implement`
where not yet real), the conformance suite run against both adapters (failing), and failing session-store /
credentials tests that inject `MemFs`.

## Green wave
**Wave:** 1 of 2
**Greenable independently:** yes — needs nothing from another node.
**Concurrent with:** n2
**Blocks:** n3–n13 (their tests exercise `MemFs` behaviour)

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Seam convention (for every migration node)

An existing free function keeps its signature and delegates to a new `<fn>_on(fs: &dyn Fs, …)`
(or `Arc<dyn Fs>` where the result keeps writing, e.g. `CredentialStore::create_on`) with `DiskFs`.
Tests call the `_on` form with `MemFs`; callers outside the migrated crate are untouched.

## Red-step state

Published surface: `tddy_fs::{Fs, FsMetadata, FsKind, DiskFs, MemFs (new, fail_next_write, paths), AsyncFs, write_atomic, write_atomic_with_mode, write_atomic_labelled}`;
`tddy_session_store::output::{write_session_file_on, read_session_file_on, create_session_dir_with_id_on}`,
`tddy_session_store::session_actions::parse_action_manifest_file_on`;
`tddy_credentials::CredentialStore::{create_on, open_with_passphrase_on}`.
26 failing tests (conformance 11, atomic_write 8, async_fs 1, session-store fs_port 3, credentials fs_port 3), each on `TODO(fs-crate)`.

**Added in pass 2 (third commit), for n3–n13:** `tddy_fs::testing::direct_disk_io_in(crate_dir) -> Vec<String>` behind
feature `testing` — the guard every migration node's `tests/fs_port.rs` runs over its own `src/` (skips `#[cfg(test)]`
items and comments). Failing tests: `packages/tddy-fs/tests/direct_disk_io.rs` (4). Each migrated crate takes
`tddy-fs = { path = "../tddy-fs", features = ["testing"] }` as a **dev-dependency**.
Pre-existing tests of the three crates pass (`./test -p` scoped: tddy-fs, tddy-session-store, tddy-credentials).

## Prerequisites
- [2026-09-23-atomic-file-leaf-crate.md](../todo/2026-09-23-atomic-file-leaf-crate.md) — ✅ RESOLVED HERE (`tddy-fs` is the leaf crate).

## Scope
- [ ] `tddy-fs` crate: trait, `DiskFs`, `MemFs`, `AsyncFs` (tokio feature)
- [ ] Move `atomic_file.rs` into `tddy-fs` (try `tddy-tools restructure` first; refusal ⇒ ask operator); re-export from session-store
- [ ] Shared conformance suite for both adapters
- [ ] `tddy-session-store` on `Arc<dyn Fs>`
- [ ] `tddy-credentials` vault on the port; delete `atomic.rs`
- [ ] Package docs for `tddy-fs`

## Technical changes
**State A:** no port; four atomic-write copies; session-store and credentials call `std::fs` directly.
**State B:** one port crate; both crates take `Arc<dyn Fs>` (constructors default to `DiskFs` at the composition sites).

## Acceptance tests
- `packages/tddy-fs/tests/conformance.rs` — `disk_fs_conforms`, `mem_fs_conforms` (read/write round-trip, missing parent created, rename overwrite, remove, read_dir order-agnostic, modes)
- `packages/tddy-fs/tests/atomic_write.rs` — `a_failed_atomic_write_leaves_the_old_contents`, `a_first_write_with_mode_is_never_wider_than_the_mode`, `no_swap_file_survives_a_failure` (MemFs fault injection: ENOSPC on write)
- `packages/tddy-session-store/tests/…` — `session_state_round_trips_through_mem_fs`
- `packages/tddy-credentials/tests/…` — `vault_is_written_owner_only_through_the_port`

Gates: `./test -p tddy-fs -p tddy-session-store -p tddy-credentials`, clippy on the same.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement
- [ ] /validate-changes
- [ ] /pr-wrap
