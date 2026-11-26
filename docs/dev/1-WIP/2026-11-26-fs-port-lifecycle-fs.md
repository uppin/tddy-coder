# Changeset: session lifecycle crates read and write disk through the FS port

**Date**: 2026-11-26
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — several crates, constructor signatures change to take `Arc<dyn Fs>`
**PRD**: [PRD-2026-11-26-fs-port-lifecycle-fs.md](../../ft/coder/1-WIP/PRD-2026-11-26-fs-port-lifecycle-fs.md)

**Sequencing (hard gate):** do not start `/green` before `#target-reconnect` #1041 and #1042 have merged — they edit agent-launch, cli-sessions and session-lifecycle, which this streak migrates. Implementation runs as `#muse` sessions on `mantas-udoo-ros` (one session at a time, tight disk): keep every gate scoped with `./test -p <crate>` per crate, never a workspace run.

## Responsibility
Production disk I/O of these crates, moved onto `tddy-fs` (~186 fs lines incl. tests):

| Crate | fs lines (src/) |
|---|---|
| `tddy-session-lifecycle` | 140 |
| `tddy-session-split` | 19 |
| `tddy-session-actions` | 12 |
| `tddy-session-agents` | 11 |
| `tddy-session-catalog` | 2 |
| `tddy-session-driver` | 2 |

## Boundaries
- Changes no on-disk format, path or mode; adds no `Fs` method (a missing one is n1's to add — raise it on n1's PR).
- Touches no crate outside the table (callers outside it only get a `DiskFs` argument at composition sites).
- Test-only crates (testing-commons, e2e, vm-testkit, test-hosts) keep real disk.

## Placement
**Streak:** `#fs-port` — n4 of 13
**Unit:** stack `fs-port` — position 3 of 12 (n1 · n3 · **n4** · n5 · n6 · n7 · n8 · n9 · n10 · n11 · n12 · n13)
**Base:** `feature/fs-port/daemon-fs` (n3)
**Why stacked:** consumes n1's `Fs`/`MemFs`. Flattened behind n3 only because `gh stack` is linear — it does not depend on n3.

## Dependencies

These surfaces are **theirs to create**; implementing one here collides with the PR that owns it.

| Node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` fs-crate | `tddy_fs::{Fs, DiskFs, MemFs, AsyncFs, write_atomic*}` | production code calls the port; tests inject `MemFs` | add methods to `Fs`, change adapters, or add another atomic writer |

## Draft PR contract
First push: constructor/function signatures of the listed crates changed to take `Arc<dyn Fs>` (bodies keep
working on `DiskFs` where unmigrated, marked `// TODO(lifecycle-fs): implement`), plus failing `MemFs`-based tests.

## Green wave
**Wave:** 2 of 2
**Greenable independently:** no — tests exercise n1's `MemFs` behaviour.
**Concurrent with:** n3–n13 (any order; serialized only by the stack line and the one-session Muse cap)
**Blocks:** nothing (successor in the line by flattening only)

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Scope
- [ ] `tddy-session-lifecycle` migrated
- [ ] `tddy-session-split` migrated
- [ ] `tddy-session-actions` migrated
- [ ] `tddy-session-agents` migrated
- [ ] `tddy-session-catalog` migrated
- [ ] `tddy-session-driver` migrated

## Acceptance tests
One `MemFs` behaviour test per crate in `packages/<crate>/tests/fs_port.rs`, named for the behaviour
(e.g. `<thing>_is_persisted_through_the_fs_port`), chosen in the red step from that crate's heaviest disk path.

Gates (scoped, cheap): `./test -p <crate>` for each crate in the table, `cargo clippy -p <crate> -- -D warnings`.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement
- [ ] /validate-changes
- [ ] /pr-wrap
