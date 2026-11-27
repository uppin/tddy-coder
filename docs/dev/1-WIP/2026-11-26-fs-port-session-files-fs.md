# Changeset: session files and worktrees go through the FS port

**Date**: 2026-11-26
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — several crates, ~209 fs lines, constructor signatures change; largest node class
**PR**: [#1049](https://github.com/uppin/tddy-coder/pull/1049)
**PRD**: [PRD-2026-11-26-fs-port-session-files-fs.md](../../ft/coder/1-WIP/PRD-2026-11-26-fs-port-session-files-fs.md)

**Sequencing (hard gate):** do not start `/green` before `#target-reconnect` #1041 and #1042 have merged — they edit agent-launch, cli-sessions and session-lifecycle, which this streak migrates. Implementation runs as `#muse` sessions on `mantas-udoo-ros` (one session at a time, tight disk): keep every gate scoped with `./test -p <crate>` per crate, never a workspace run.

## Responsibility
Production disk I/O of these crates, moved onto `tddy-fs` (~209 fs lines incl. tests):

| Crate | fs lines (src/) |
|---|---|
| `tddy-session-files` | 116 |
| `tddy-worktree-service` | 48 |
| `tddy-session-worktree` | 33 |
| `tddy-subagent-worktree` | 7 |
| `tddy-session-sync` | 5 |

Plus two additive port methods this node owns (operator decision 2026-11-27, as n3's `Fs`
expansion), in one surface commit ahead of the crate ports:
- `Fs::create_new` — exclusive create, `AlreadyExists` when anything exists there — with `DiskFs`
  + `MemFs` implementations and conformance tests. The attachment writers need a single atomic
  check-and-create (`create_new` open semantics) that the port did not have; emulating it with
  check-then-write would trade the atomicity the writers document for a race.
- `Fs::copy` — file copy, `IsADirectory` on a directory source on both adapters — with `DiskFs`
  + `MemFs` implementations, an `AsyncFs::copy` facade, and conformance tests. The worktree
  scratch-index seeding copies an index file; read+write would lose the source's mode and the
  single-step failure shape, so the port owns the operation instead.

## Boundaries
- Changes no on-disk format, path or mode; adds exactly two `Fs` methods (`create_new` +
  `copy`, in this node's surface commit — n1 is landed, so additive surface grows on the consuming
  node by operator consent).
- Touches no crate outside the table (callers outside it only get a `DiskFs` argument at composition sites).
- Test-only crates (testing-commons, e2e, vm-testkit, test-hosts) keep real disk.

## Placement
**Streak:** `#fs-port` — n5 of 13
**Unit:** stack `fs-port` — position 4 of 12 (n1 · n3 · n4 · **n5** · n6 · n7 · n8 · n9 · n10 · n11 · n12 · n13)
**Base:** `feature/fs-port/lifecycle-fs` (n4)
**Why stacked:** consumes n1's `Fs`/`MemFs`. Flattened behind n4 only because `gh stack` is linear — it does not depend on n4.

## Dependencies

These surfaces are **theirs to create**; implementing one here collides with the PR that owns it.

| Node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` fs-crate (#1045) | `tddy_fs::{Fs, DiskFs, MemFs, AsyncFs, write_atomic*}` | production code calls the port; tests inject `MemFs` | change adapters (except the one additive `create_new` this node owns, recorded above), or add another atomic writer |

## Draft PR contract
First push: constructor/function signatures of the listed crates changed to take `Arc<dyn Fs>` (bodies keep
working on `DiskFs` where unmigrated, marked `// TODO(session-files-fs): implement`), plus failing `MemFs`-based tests.

## Green wave
**Wave:** 2 of 2
**Greenable independently:** no — tests exercise n1's `MemFs` behaviour.
**Concurrent with:** n3–n13 (any order; serialized only by the stack line and the one-session Muse cap)
**Blocks:** nothing (successor in the line by flattening only)

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Scope
- [ ] `tddy-session-files` migrated
- [ ] `tddy-worktree-service` migrated
- [ ] `tddy-session-worktree` migrated
- [ ] `tddy-subagent-worktree` migrated
- [ ] `tddy-session-sync` migrated

## Red-step state

Every crate in the table has `tests/fs_port.rs` with `production_code_reaches_disk_only_through_the_fs_port`
(guard over its own `src/` via n1's `tddy_fs::testing::direct_disk_io_in`) and `tddy-fs` (feature `testing`) as a
dev-dependency. Behaviour seams published (`<fn>_on(&dyn Fs, …)`, bodies `TODO`): in `tddy-session-files` — see each crate's `tests/fs_port.rs`. The remaining crates' `MemFs` behaviour tests are chosen during `/green`, per the PRD's
"one behaviour test per crate" criterion.

Red run (`cargo test -p <each crate> --test fs_port`): 7 failing, 0 passing. Today every failure stops on n1's
`TODO(fs-crate)`; once n1 is green, each guard fails on this crate's own direct disk I/O and each seam test on
`TODO(<this node>)`. Build: `cargo clippy --all-targets -- -D warnings` clean on every crate in the table.
User review: operator approved n1's tests and pre-approved the rest ("approve and finish the rest without stopping", 2026-11-26).

## Acceptance tests
One `MemFs` behaviour test per crate in `packages/<crate>/tests/fs_port.rs`, named for the behaviour
(e.g. `<thing>_is_persisted_through_the_fs_port`), chosen in the red step from that crate's heaviest disk path.

Gates (scoped, cheap): `./test -p <crate>` for each crate in the table, `cargo clippy -p <crate> -- -D warnings`.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement
- [ ] /validate-changes
- [ ] /pr-wrap
