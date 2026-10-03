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

The create-pane test attaches one file, because the form streams a start only when something is
attached (no-attachment starts stay on unary `StartSession`, pinned by
`CreateSessionAttachmentProgress.cy.tsx`). Whether every start should stream so phases show without
attachments is a **green decision** that would change that pinned test — flagged, not decided here.

## Technical Debt & Production Readiness

**Stubs (`TODO(indexing-indicators)`) published by the draft-PR contract:**

- `tddy-session-files` `AttachmentProgressSink::{begin_phase, end_phase}` exist and send
  `StartSessionEvent.phase`; **no start calls them** (`svc_start_session_core.rs` doc,
  `claude_cli_spawn.rs` at the worktree cut), so today's stream is byte-identical. Green threads the
  sink into each session type's start (claude-cli, cursor-cli, workspace, tool, sandboxed variants).
- `tddy-daemon` `code_index_warmup::warm_for_session` returns `None` and starts nothing;
  `SessionIndexProgress` (per-session `watch` channel of the latest `CodeIndexProgress`) is
  implemented — it is a holder, not behaviour.
- `CodeNavigationServiceImpl::watch_code_index` answers `Unimplemented`; `index_progress` is
  `#[allow(dead_code)]` until it reads it.
- `runtime.rs`: building one `SessionIndexProgress`, handing it to the service and calling
  `warm_for_session` when a started session's worktree exists — a comment only. **Open design
  point for green:** the start lives in `tddy-session-lifecycle`, which cannot name `tddy-daemon`;
  the trigger needs a port on `DaemonSessionHost` (a "session worktree ready" observer the runtime
  installs) — not drafted here, to avoid inventing a signature no test pins.
- `WatchCodeIndex` authorisation (token → OS user → the session is theirs) is unwritten.
- Web: `useSessionAttachments.startPhase` is always `null`; `SessionIndexingIndicator` renders
  `null`; `SessionMainPane` carries a TODO where the header renders it. `CreateSessionStartPhase`
  (the text per step) is implemented.

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

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## TODO

- [x] Record initial discovery (`2026-10-03-indexing-indicators-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-indexing-indicators-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
