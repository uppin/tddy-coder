# Changeset: Session start and indexing progress in the session UI

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-indexing-indicators-initial-discovery.md).

## Stack

`#live-plan` 12/15 — branch `feature/live-plan/indexing-indicators`, base `feature/live-plan/session-lsp-tools`.
PR: [#571](https://github.com/uppin/tddy-coder/pull/571)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- `StartPhase` events in `StreamStartSession`.
- Background `Warm` per session worktree and `WatchCodeIndex`.
- The two web indicators.

## Boundaries

- Does **not** block session start on indexing.
- Does **not** change semantic indexing itself.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `code-navigation` (10) | `CodeNavigationService` and the `connect`-based forwarding helper | `WatchCodeIndex` is a call on that service; warm goes through the same helper | add a second service or forwarding path |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

**Sequencing facts (recorded at the draft-PR contract, 2026-10-03).**

- The contract commit sits on `session-lsp-tools`' **planning** commit (`f5f84174`), not on its
  contract commit (`6dbaf17b`), which landed on that branch after this one was cut — the same shape
  `session-lsp-tools` itself has over `transactional-groups`. The closing cascade `/pr-stack-rebase`
  brings it in. Expected friction there: `6dbaf17b` adds `Symbols` / `Diagnostics` to
  `code_index.proto`'s `CodeIndexService`, so `tests/code_index_warmup_acceptance.rs`'s fake index
  must gain those two methods (answering `not_part_of_this_fake()`), exactly as that commit did to
  `code_navigation_acceptance.rs`'s fake; both commits also touch `runtime.rs` (different blocks).
- `code-navigation`'s `CodeNavigationServiceImpl` is still all stubs at this base; `WatchCodeIndex`
  is an **addition** to it (one field, `index_progress`, a `with_index_progress` builder and the new
  method) — `new(...)` and the three existing methods are untouched. `WatchCodeIndex`'s green needs
  nothing of code-navigation's green except the service being registered (it already is).
- The warm goes through `IndexDaemonRegistry::connect` (code-navigation's forwarding helper);
  no second forwarding path.
- Start phases need nothing from any parent.

## Draft PR contract

The first push after this commit publishes: `session.proto` `StartPhase` + `StartSessionEvent.phase`; `code_navigation.proto` `WatchCodeIndex`; daemon `code_index_warmup::warm_for_session` — `TODO(indexing-indicators)`; web `SessionIndexingIndicator`; failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** partly — start-phase tests need nothing; `WatchCodeIndex` needs `code-navigation`'s service green.
**Concurrent with:** transactional-groups, session-lsp-tools
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/plan-dialog` — next in the line.

## Prerequisites

### ℹ REFERENCE — warm ready means a live server, not a loaded graph — [`2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md`](../todo/2026-09-16-warm-ready-means-a-live-server-not-a-loaded-graph.md)

The indicator reads `Warm`'s `ready`, which waits for the graph; the registry's own readiness (socket bound) is not used for it.

## Affected Packages

- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `StartPhase` in `session.proto`; `WatchCodeIndex` on `code_navigation.proto`
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) — phase events around worktree, semantic index, agent
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — warm on session worktree start; per-session latest progress
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — phase text in the create pane; indexing indicator in the session header

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

Session start is one call; worktree creation and the optional semantic index block it; `StartSessionEvent` carries only attachment progress and the result; the web shows a disabled Create button and attachment rows; `SessionEntry` has no index readiness; `Warm` streams `IndexProgress{line, phase, percentage, furthest, ready}`.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

All fully implemented; every one fails on **this node's** missing behaviour, none on a parent's.

### tddy-session-lifecycle — `tests/start_phase_acceptance.rs`

Harness: a real in-process `claude-cli` start over `StreamStartSession` (a project repo whose
`origin` is itself, `/bin/cat` as `claude` — the `claude_cli_session_acceptance.rs` fixture), the
session type the create pane starts most and the one with all three steps.

| Test | Line | Fails because |
|---|---|---|
| `starting_a_session_streams_worktree_then_agent_phases_before_the_result` | :216 | stream says only `[Result]`; expected `Began(Worktree), Ended(Worktree), Began(Agent), Ended(Agent), Result` — no phase is emitted yet (`TODO(indexing-indicators)` in `claude_cli_spawn.rs`) |
| `semantic_index_phase_is_streamed_when_enabled` | :242 | stream says only `[Failed(FailedPrecondition)]`; expected `Began(Worktree), Ended(Worktree), Began(SemanticIndex), Failed(FailedPrecondition)` |

The semantic-index test asserts the step **beginning** and the start then failing: the crate's tests
build without the `local-model` feature, so `index_session_worktree` refuses at once ("semantic index
requested but no embedder is available") — the existing `workspace_session_start_acceptance.rs`
relies on the same refusal. A failed step sends no END (`StartPhase` doc in `session.proto`).

### tddy-daemon — `tests/code_index_warmup_acceptance.rs`

Harness: `code_navigation_acceptance.rs`'s — a shell-script stand-in index daemon symlinked to a
fake tonic `code_index` server — with a fake whose `Warm` stream the test drives. The warm is started
by calling `code_index_warmup::warm_for_session` directly (the seam a started session's worktree
reaches); the session-start → warm hook is a `runtime.rs` TODO (see Technical Debt).
`WatchCodeIndex` is dispatched at the registered coordinate (`entry.service.handle_rpc`).

| Test | Line | Fails because |
|---|---|---|
| `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists` | :462 | `warm_for_session` stub returns `None` — "a session on a Rust worktree starts a warm" |
| `watch_code_index_delivers_phase_percentage_and_ready` | :490 | same stub (no warm to join at 40%) |
| `without_an_index_daemon_no_warm_starts` | :516 | its first half (no warm, no progress) passes on the stub as a guard; its second half — `WatchCodeIndex` ends empty — fails: `Unimplemented "WatchCodeIndex is not served yet — TODO(indexing-indicators)"` |
| `a_warm_failure_is_reported_and_the_session_stays_usable` | :536 | same `warm_for_session` stub |

`tests/test_placement.rs` registers the new suite in `BELONGS_HERE` (passes).

### tddy-web — `cypress/component/SessionStartAndIndexingProgress.cy.tsx`

| Test | Line | Fails because |
|---|---|---|
| `the create pane shows the current start phase` | :235 | `[data-testid='create-session-start-phase']` never appears — `useSessionAttachments.startPhase` is always `null` |
| `the session header shows indexing until ready` | :252 | `[data-testid='session-indexing-indicator']` never appears — `SessionIndexingIndicator` renders `null` |

The create-pane test attaches one file, because at the contract the form streamed a start only when
something was attached. **Decided in green:** every start streams, so phases show without
attachments; the pinned no-attachment test in `CreateSessionAttachmentProgress.cy.tsx` now pins the
streamed start instead of the unary one.

## Technical Debt & Production Readiness

**Stubs published by the draft-PR contract — state after green:**

- ✅ Start phases: claude-cli, cursor-cli and workspace starts report worktree / semantic index /
  agent through `AttachmentProgressSink::{begin_phase, end_phase}`. ⚠ The sandboxed claude-cli and
  cursor-cli, tool and split starts do **not** yet — `TODO(indexing-indicators)` in
  `svc_start_session_core.rs`.
- ✅ `code_index_warmup::warm_for_session` warms through the `IndexChannelSource` port (the base's
  `tddy-daemon-rpc` port, which the daemon's `IndexDaemonRegistry` implements); a failed or short warm
  ends with `error` set; a first `Starting` record is written before the task spawns. The module lives
  in `tddy-daemon-rpc`, beside the navigation service that reads its progress.
- ✅ `watch_code_index` follows `SessionIndexProgress` to `ready` / `error`; `index_progress`'s
  `#[allow(dead_code)]` is gone.
- ✅ Runtime hook: new port `SessionWorktreeObserver` (`tddy-session-lifecycle`), installed with
  `DaemonSessionHost::with_worktree_observer`, implemented by the daemon's `IndexWarmupObserver`;
  installed only when `index_daemon:` is configured. The index-daemon block in `runtime.rs` moved
  above the session host so the observer can hold the registry (same task registry as before).
- ✅ `WatchCodeIndex` authorisation: token → OS user → `<sessions base>/sessions/<id>` must exist
  (`WorktreeServiceImpl::resolve_owned_session_dir`, which `restore_session_worktree` shares through
  `session_dir_for`, so there is one ownership model). A foreign session and a missing one are the
  same `NotFound`. `SessionIndexProgress::follow` is a non-creating lookup, so watching an unknown id
  records nothing. Pinned by `another_users_watch_of_the_session_is_refused_and_shows_no_progress`;
  the suite's fixture now creates the session directory. `authorize` stays private.
- ✅ Web: `startPhase` follows the stream's `phase` events; `SessionIndexingIndicator` follows
  `watchCodeIndex`, shows `error`, cleans up on unmount; rendered in `SessionMainPane`'s header
  through `codeNavigationClient` (`SessionsDrawerScreen` passes the owning host's client).
- 🆕 **Every start now streams** (developer decision): `CreateSessionPane` always uses
  `startSessionStreamed`; `CreateSessionAttachmentProgress.cy.tsx`'s pinned unary test was rewritten
  to pin the streamed start. Component specs that stub only unary `startSession` keep working through
  `registerServerStreamFallback` in `tddy-connectrpc-testkit` (registered in `cypress/support/component.ts`).
- 🆕 `session_coordinate_handlers.rs`: the terminal event is sent only after the progress forwarding
  task has drained, so `Result` can no longer overtake a phase's `Ended`.

**Not written, and why:**

- No unit tests: every behaviour here is a seam between processes or a stream; the acceptance
  tests pin each at its narrowest reachable seam.
- No test drives a **real** session start into a warm (lifecycle → daemon): that hook's port does
  not exist yet (above), and pinning it now would fabricate its signature.
- `WatchCodeIndex` authorisation refusals are not pinned: the rule (session ownership vs. token
  only) is the green phase's to settle with the hook.

## Decisions & Trade-offs

- **`StartPhase { Step step; Boundary boundary; }`** — two enums rather than a message per step: one
  oneof arm on `StartSessionEvent` (field 3), and a consumer tracks "the last BEGIN without its END".
  A failed step sends no END; the stream's error is the end.
- **Phases ride the existing progress sink** (`AttachmentProgressSink`) rather than a second sink:
  it already reaches every session type's start as `progress`, and unary `StartSession` keeps
  discarding everything.
- **`CodeIndexProgress` is `IndexProgress` plus `error`**, a new message on `code_navigation.proto`
  rather than an import of `code_index.proto`: tddy-service does not compile the index daemon's
  proto, and the web reads only tddy-service's.
- **`warm_for_session` is a free function returning `Option<JoinHandle<()>>`** — `None` is "nothing
  started" (no `index_daemon:`, or no `Cargo.toml` at the worktree root), the handle lets a caller
  (and a test) await a warm that is otherwise detached.

## Restructuring

`tests/unbundle_endpoint.rs::every_module_left_in_the_daemon_is_one_of_the_endpoint_set` lets
`tddy-daemon/src` hold wiring only. After the base was rewritten (its navigation service already moved
to `tddy-daemon-rpc` behind an `IndexChannelSource` port) the one file this PR would have added to that
list was its own `code_index_warmup.rs`; the parent-owned `index_daemon/lsp_channel.rs` was the other,
and the parent resolved it itself (#live-plan 11/15, `ec72da38`: it names the module among the endpoint's
wiring modules, with its own rationale). This PR therefore leaves `index_daemon/` untouched.

- `code_index_warmup.rs` is born in `tddy-daemon-rpc/src/` (its progress holder is read by the
  navigation service there, and that crate cannot depend on the daemon); it dials through the port, and
  its acceptance suite stays in `tddy-daemon/tests` beside `code_navigation_acceptance.rs`, which needs
  the daemon's registry. `tddy-daemon` gains no non-wiring module from this PR.

Result (scoped): `unbundle_endpoint` 4/4, `test_placement` 4/4, `code_index_warmup_acceptance` 6/6,
`code_navigation_acceptance` 7/7, clippy `-D warnings` clean.

An earlier plan to move the whole registry cluster to a crate with the restructure engine, and a
fold of `lsp_channel.rs` into `registry.rs`, were both dropped once the parent whitelisted the file;
the engine defect the move hit is recorded in
[`docs/dev/todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md`](../todo/2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md).

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes (2026-10-03, head `e05c49bc`)

**Stack gate:** current with `feature/live-plan/session-lsp-tools`; `origin/<base>..HEAD` is this PR's six commits only; no deletions; nothing from `## Dependencies` reimplemented (`WatchCodeIndex` is the addition the changeset names). **Build:** `cargo build -p tddy-daemon -p tddy-session-lifecycle -p tddy-worktree-service -p tddy-session-files -p tddy-service` clean, no warnings (scoped).

**Tests (scoped):** `start_phase_acceptance` 2/2, `code_index_warmup_acceptance` 4/4 (re-run); web: `SessionStartAndIndexingProgress`, `CreateSessionAttachmentProgress`, `CreateSessionAcceptance`, `CreateSessionBranchConflictAcceptance`, `CreateSessionAutoClosesDrawer`, `PrStackStartSessionModalAcceptance`, `CreateSessionCodebaseHostAcceptance` all pass. ⚠ Not compared with a clean base: `./test -p tddy-session-lifecycle` 22 failures (16 sandboxed-session / macOS sandbox-bridge gap, 6 `session_sync_livekit_acceptance`), `./test -p tddy-daemon` 1 failure (`unbundle_endpoint`, names modules this round did not add). ~30 other specs using `startSession` are left to CI.

| Severity | Where | Finding |
|---|---|---|
| ✅ fixed | `code_navigation.rs` `watch_code_index` | was token-only; now refuses a session that is not the caller's (see Technical Debt) |
| WARNING → deferred | `svc_start_session_core.rs:58` | own `TODO(indexing-indicators)`: sandboxed, tool and split starts report no phases and trigger no warm — recorded in `docs/dev/todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md` (developer chose to defer) |
| ✅ fixed | `code_index_warmup.rs` `SessionIndexProgress` | watching an unknown id no longer creates an entry (`follow`); entries are still never freed for warmed sessions |
| INFO | `./test -p tddy-worktree-service` | `worktree_size_calculator_acceptance::a_cached_size_is_served_after_reload_without_recomputing` and `remote_git_livekit_acceptance` fail; neither touches `authorize` / `restore_session_worktree`; not compared with the base |
| INFO | `tddy-connectrpc-testkit` | outside the planned surface; test infrastructure, process-wide registration |
| INFO | `CreateSessionAcceptance.cy.tsx` | failed 15/15 once in a batch run during a full-disk episode, passed alone; cause not found |

### /validate-tests (2026-10-03, head `399c18dc`)

Analysed 13 tests in 4 files: `start_phase_acceptance.rs` (2), `code_index_warmup_acceptance.rs` (5), `SessionStartAndIndexingProgress.cy.tsx` (2), `CreateSessionAttachmentProgress.cy.tsx` (4, one rewritten in green). No `#[ignore]`, `.skip`/`.only`, sleeps, `cy.intercept` or raw `data-testid` selectors; no always-passing tests. The new `another_users_watch_of_the_session_is_refused_and_shows_no_progress` and the rewritten no-attachment Cypress test are compliant Given/When/Then with named helpers.

| Severity | Where | Finding |
|---|---|---|
| WARNING | `code_index_warmup_acceptance.rs` `without_an_index_daemon_no_warm_starts` | two behaviours in one test (no warm starts; watching ends empty) — split |
| WARNING | `code_index_warmup_acceptance.rs` `a_warm_failure_is_reported_and_the_session_stays_usable` | name promises "the session stays usable"; the body asserts only the reported failure — name states what it pins |
| ✅ fixed | `tddy-connectrpc-testkit` `registerServerStreamFallback` | had no tests of its own; now `cypress/component/ServerStreamFallback.cy.tsx` (7 tests) pins the unary answer through both registration paths, an explicit stream winning, the error code, the recording and the no-fallback case — driven from tddy-web's Cypress because the testkit package has no runner |
| INFO | `CreateSessionAttachmentProgress.cy.tsx` rewritten test | asserts the phase text and the created session in one scenario, and builds the `phase` wire event inline; the other tests in the file do the same |
| INFO | web | no test pins END clearing the phase or a stream error ending it |

### /validate-prod-ready (2026-10-04, head `644cb0a2`)

Scanned the 700 lines this PR adds to 26 production files (tests, generated code and Cypress support excluded) against the base. No mock or fake code, no debug output (`println!`/`dbg!`/`console.*`), no environment-conditional behaviour, no fallbacks; the `#[allow(dead_code)]` the contract commit carried is gone. One `TODO(indexing-indicators)` remains (`svc_start_session_core.rs:58`), pointing at its deferral entry; three `.expect("the session progress map")` guard a mutex that can only be poisoned by a panic while held.

| Severity | Where | Finding |
|---|---|---|
| ✅ fixed | `code_index_warmup.rs` `SessionIndexProgress::watch` | the entry-creating subscribe had no production caller (only a test helper); removed, the helper uses `follow` |
| ✅ fixed | `session_coordinate_handlers.rs` | `let _ = forwarding.await` discarded a `JoinError`; now logged as a warning, ordering guarantee unchanged |
| INFO | `code_index_warmup.rs` `SessionIndexProgress` | entries for warmed sessions are never freed |

### File-length gate (/pr-wrap step 3.5, 2026-10-04, base `c3567fde`)

Production lines, counted to the first `#[cfg(test)]`; every deferral below has the developer's consent and a record.

| File | Before → after | Outcome |
|---|---|---|
| `tddy-session-lifecycle/src/cursor_cli_spawn.rs` | 445 → 532 | 🔴 **this PR crossed 500** — deferred with consent: `docs/dev/todo/2026-10-04-cursor-cli-spawn-crossed-the-file-budget-in-indexing-indicators.md`; code-issue record regressed |
| `tddy-daemon/src/runtime.rs` | 1650 → 1668 | 🔴 grew — stack rule (dependent #572 and parents touch it): record updated, split deferred to after the stack lands |
| `tddy-worktree-service/src/service.rs` | 752 → 777 | 🔴 grew — stack rule (#572 touches it): record updated |
| `tddy-web/.../CreateSessionPane.tsx` | 783 → 785 | 🔴 grew — stack rule: new record `oversized-file-create-session-pane.md` |
| `tddy-web/.../SessionMainPane.tsx` | 666 → 674 | 🔴 grew — stack rule: new record `oversized-file-session-main-pane.md` |

`tddy-rust-typescript-tests/gen/session_pb.ts` (1540 → 1640) is generated `buf` output and excluded by design.

### /analyze-clean-code (2026-10-04, head `c75381a4`)

Measured every Rust function overlapping a line this PR adds (script: function extent by brace matching, parameters by top-level commas, nesting by brace depth; production code only) plus the TypeScript by reading. **Raw rubric score: F** — eleven touched functions are over a limit — **but none is a function this PR wrote**: every "must refactor" item is a pre-existing function this PR extended. The functions this PR wrote are all within limits.

| Metric | Excellent | Acceptable | Needs attention | Must refactor |
|---|---|---|---|---|
| Function length (touched, Rust) | 17 | 3 | 3 | 8 |
| Parameter count | — | — | — | 3 (`SessionHost::new` 8, `cursor_cli_semantic_env` 7, `spawn_cursor_cli_session_inner` >5) |

**Written by this PR, all within limits:** `warm_for_session` (36 lines, 4 params), `warm` (30, 4), `resolve_owned_session_dir` (12), `follow`, `latest`, `record`, `web_progress`, `announce_worktree_ready`; `watch_code_index` is 37 lines at nesting 4 (needs attention: the spawned follow loop).

**Pre-existing must-refactor functions this PR made longer** (lines added by this PR in brackets): `runtime.rs::build` 928 [+39], `svc_start_session_core.rs::start_session_core` 373 [+19], `cursor_cli_spawn.rs::spawn_cursor_cli_session_reporting` 251 [+50, the phase reporting around the former inner function], `claude_cli_spawn.rs::spawn_claude_cli_session_inner` 263 [+8], `session_coordinate_handlers.rs::stream_start_session_at_session_coordinate` 91 [+11], `cursor_cli_spawn.rs::spawn_cursor_cli_session_inner` [+29, a pass-through wrapper over many parameters], `cursor_cli_semantic_env` (6 → 7 parameters, `progress` added). Others touched by one to three lines: `spawn_child` 116, `start_claude_cli_session` 104, `SessionHost::new` 92 (8 parameters), `spawn_conversation` 74, `managed_claude_cli_launch` 62.

**Magic values:** none in what this PR adds (the watch buffer is the named `WATCH_BUFFER`). **Naming:** no single-letter names. **Duplication:** the `clone sessions_base / clone session_id → start → announce_worktree_ready` block appears twice in `svc_start_session_core.rs` (claude-cli, cursor-cli) — a small helper candidate, left as is. **Oversized files:** see the file-length gate above (all deferred).

Not refactored here: the long functions are the same ones the file-length records and the restructure engine already track; splitting them in a stacked PR would collide with #572 and the parents.

## TODO

- [x] Record initial discovery (`2026-10-03-indexing-indicators-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (⚠ sandboxed / tool / split starts deferred to `docs/dev/todo/` — see Technical Debt)
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest — local scoped run done; CI not yet read
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready)
- [x] Refactor production readiness issues
- [x] Analyze code quality (/analyze-clean-code)
- [x] Refactor code quality issues — none applied; developer chose report-only: the over-limit functions are pre-existing, tracked by the file-length records, and a split would collide with #572
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-indexing-indicators-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
