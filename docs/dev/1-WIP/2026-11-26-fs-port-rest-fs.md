# Changeset: every remaining production crate goes through the FS port

**Date**: 2026-11-26
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — several crates, ~333 fs lines, constructor signatures change; largest node class
**PR**: [#1057](https://github.com/uppin/tddy-coder/pull/1057)
**PRD**: [PRD-2026-11-26-fs-port-rest-fs.md](../../ft/coder/1-WIP/PRD-2026-11-26-fs-port-rest-fs.md)

**Sequencing (hard gate):** do not start `/green` before `#target-reconnect` #1041 and #1042 have merged — they edit agent-launch, cli-sessions and session-lifecycle, which this streak migrates. Implementation runs as `#muse` sessions on `mantas-udoo-ros` (one session at a time, tight disk): keep every gate scoped with `./test -p <crate>` per crate, never a workspace run.

## Responsibility
Production disk I/O of these crates, moved onto `tddy-fs` (~333 fs lines incl. tests):

| Crate | fs lines (src/) |
|---|---|
| `tddy-zed-extensions` | 117 |
| `tddy-coder` | 57 |
| `tddy-tools` | 24 |
| `tddy-presenter` | 18 |
| `tddy-telegram-control` | 10 |
| `tddy-toolcall` | 9 |
| `tddy-service` | 9 |
| `tddy-model-registry` | 8 |
| `tddy-tui` | 6 |
| `tddy-livekit` | 6 |
| `tddy-screen-sharing` | 5 |
| `tddy-graph` | 5 |
| `tddy-acp-stub` | 4 |
| `tddy-telegram` | 3 |
| `tddy-terminal-rpc` | 2 |
| `tddy-vm-build` | 2 |
| `tddy-tools-cli` | 1 |
| `tddy-session-sync-cli` | 1 |
| `tddy-livekit-screen-capture` | 1 |
| `tddy-coder-cli` | 1 |
| `tddy-work-queue` | 22 |
| `tddy-sandbox-qemu` | 5 |
| `tddy-build` | 17 |

## Boundaries
- Changes no on-disk format, path or mode; adds no `Fs` method (a missing one is n1's to add — raise it on n1's PR).
- Touches no crate outside the table (callers outside it only get a `DiskFs` argument at composition sites).
- Test-only crates (testing-commons, e2e, vm-testkit, test-hosts) keep real disk.

## Placement
**Streak:** `#fs-port` — n13 of 13
**Unit:** stack `fs-port` — position 12 of 12 (n1 · n3 · n4 · n5 · n6 · n7 · n8 · n9 · n10 · n11 · n12 · **n13**)
**Base:** `feature/fs-port/hosts-fs` (n12)
**Why stacked:** consumes n1's `Fs`/`MemFs`. Flattened behind n12 only because `gh stack` is linear — it does not depend on n12.

## Dependencies

These surfaces are **theirs to create**; implementing one here collides with the PR that owns it.

| Node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` fs-crate (#1045) | `tddy_fs::{Fs, DiskFs, MemFs, AsyncFs, write_atomic*}` | production code calls the port; tests inject `MemFs` | add methods to `Fs`, change adapters, or add another atomic writer |

## Draft PR contract
First push: constructor/function signatures of the listed crates changed to take `Arc<dyn Fs>` (bodies keep
working on `DiskFs` where unmigrated, marked `// TODO(rest-fs): implement`), plus failing `MemFs`-based tests.

## Green wave
**Wave:** 2 of 2
**Greenable independently:** no — tests exercise n1's `MemFs` behaviour.
**Concurrent with:** n3–n13 (any order; serialized only by the stack line and the one-session Muse cap)
**Blocks:** nothing (successor in the line by flattening only)

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Scope
- [ ] `tddy-zed-extensions` migrated
- [ ] `tddy-coder` migrated
- [ ] `tddy-tools` migrated
- [ ] `tddy-presenter` migrated
- [ ] `tddy-telegram-control` migrated
- [ ] `tddy-toolcall` migrated
- [ ] `tddy-service` migrated
- [ ] `tddy-model-registry` migrated
- [ ] `tddy-tui` migrated
- [ ] `tddy-livekit` migrated
- [ ] `tddy-screen-sharing` migrated
- [ ] `tddy-graph` migrated
- [ ] `tddy-acp-stub` migrated
- [ ] `tddy-telegram` migrated
- [ ] `tddy-terminal-rpc` migrated
- [ ] `tddy-vm-build` migrated
- [ ] `tddy-tools-cli` migrated
- [ ] `tddy-session-sync-cli` migrated
- [ ] `tddy-livekit-screen-capture` migrated
- [ ] `tddy-coder-cli` migrated
- [ ] `tddy-work-queue` migrated
- [ ] `tddy-sandbox-qemu` migrated
- [ ] `tddy-build` migrated

## Red-step state

Every crate in the table has `tests/fs_port.rs` with `production_code_reaches_disk_only_through_the_fs_port`
(guard over its own `src/` via n1's `tddy_fs::testing::direct_disk_io_in`) and `tddy-fs` (feature `testing`) as a
dev-dependency. No behaviour seam was published in the red step; The remaining crates' `MemFs` behaviour tests are chosen during `/green`, per the PRD's
"one behaviour test per crate" criterion.

Red run (`cargo test -p <each crate> --test fs_port`): 23 failing, 0 passing. Today every failure stops on n1's
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
