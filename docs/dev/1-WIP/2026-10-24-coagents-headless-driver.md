# Changeset: Headless `-p` coding agents as subagents

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — two crates, three spawned-process seams, permission routing through an MCP tool into n3's yield, per-CLI confinement facts partly unconfirmed
**Branch**: `feature/coagents/headless-driver`, base `feature/coagents/remote-codebase` (#725) — `#coagents` 7/8
**PR**: _assigned at wave 1_

## Initial Discovery

[2026-10-24-coagents-headless-driver-initial-discovery.md](./2026-10-24-coagents-headless-driver-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ℹ ANSWERED — Muse has no headless backend — [`2026-10-22-muse-code-headless-backend.md`](../todo/2026-10-22-muse-code-headless-backend.md)

Muse stays out; the entry is unchanged.

### ⚠ DURING — `backends` file budget — `packages/tddy-agent-backend/docs/code-issues/`

The headless driver goes in a new module; `claude.rs`, `cursor.rs`, `codex.rs` grow only by the
helpers they expose (argv builders, stream parsers made reusable).

## Responsibility

- `tddy_agent_backend::headless_driver::{HeadlessDriver, HeadlessHarness { Claude, Cursor, Codex }, HeadlessTurn}` — one turn = one CLI invocation with the conversation's session id (fresh → `--session-id`/none; then `--resume` / `exec resume`), stream → `AcpUpdate`s (n1's shape), cancel → kill the child, drop → kill.
- A `ConversationDriver` trait over `AcpDriver` and `HeadlessDriver` that `CodingAgentSession` holds (n2's session generalised; the trait lives with the drivers).
- Claude permission routing: `--permission-prompt-tool mcp__tddy-tools__approval_prompt` pointed at a per-conversation `tddy-tools` endpoint that parks the request into n3's `ParkedPermissions`.
- Catalog: `claude`, `cursor`, `codex` rows with `transport` (`acp` | `headless`) and `permission_policy` (`asks_caller` | `fixed:<text>`) on `SubagentInfo` / `SessionAgentEntry`.
- Confinement table rows for the three CLIs.
- Web: transport and policy text on the picker row.

## Boundaries

- No Muse. No change to the workflow backends' flags or behaviour. No new ACP behaviour.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `AcpDriver`, `AcpUpdate`, `AcpTurnEnd` | `ConversationDriver` abstracts over it; headless emits the same `AcpUpdate` | change `AcpDriver` |
| n2 `acp-subagent` (#721) | `CodingAgentSession`, catalog, `CodingAgentLauncher`, `HarnessConfinement` | generalises the session to the trait; adds rows | change kind fields |
| n3 `permission-yield` (#722) | `ParkedPermissions`, `StopReason::PermissionRequested` | claude's prompt tool parks into it | change the yield or the answer |
| n5, n7, n8 | inherited | transcripts / placement / relay apply unchanged | — |

## Draft PR contract

`HeadlessDriver`, `HeadlessHarness`, `ConversationDriver`, the catalog fields, the permission-tool
endpoint signature — bodies `TODO(headless-driver)` — and the failing tests below.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — needs n3's permission yield behaviour (claude routing test) and n2's session
**Concurrent with:** n6 `web-approvals`, n8 `remote-codebase`
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6      n7 → n8

## Affected Packages

- **tddy-agent-backend**: `headless_driver` module; `ConversationDriver`; reusable parsers / argv
- **tddy-coding-subagent**: session over the trait; claude permission endpoint; catalog + confinement rows
- **tddy-service**: `SubagentInfo` / `SessionAgentEntry` `transport`, `permission_policy` (regenerated TS)
- **tddy-web**: picker row text

## Related Feature Documentation

- [PRD-2026-10-24-coagents-headless-driver.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md)

## Summary

A headless transport behind the same conversation interface; `claude`, `cursor`, `codex` become
coding subagents.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `backend/claude.rs:69-139` `build_claude_args` (`-p … --output-format stream-json --verbose`, `--resume`, `--permission-prompt-tool`, `--mcp-config`, `--allowedTools`); `:549-565` `process_ndjson_stream`.
- `backend/cursor.rs:177-227` argv (`-p … stream-json --stream-partial-output --force --trust --approve-mcps`, `--resume`), writes `<wd>/.cursor/mcp.json`.
- `backend/codex.rs:421-464` `codex exec [-C wd] -s … --json [resume <thread>]`; parsed after exit (`:284-314`).
- Global `CHILD_PID` + SIGKILL (`mod.rs:168-193`) — one child per process; a per-conversation driver needs its own handle.

### State B (Target)

```
CodingAgentSession ─► dyn ConversationDriver ─┬─ AcpDriver (n1)
                                              └─ HeadlessDriver ─ claude -p | cursor agent -p | codex exec --json
                                                      claude permission prompt ─► tddy-tools approval_prompt ─► ParkedPermissions (n3)
```

## Testing Plan

**Level:** integration with **fake CLIs** — small scripts under `tddy-testing-commons` that emit
recorded stream-json / JSONL fixtures and record their argv, put on `PATH` via the harness env
overrides (`TDDY_CURSOR_AGENT`, a `claude` path override, `codex` path). Real CLIs are not run in CI.

## Acceptance Tests

`packages/tddy-coding-subagent/tests/headless_subagent_acceptance.rs` (new):

1. `a_claude_turn_returns_its_final_text_and_the_next_turn_resumes_its_session`
2. `a_cursor_turn_returns_its_final_text_and_the_next_turn_resumes_its_chat`
3. `a_codex_turn_returns_its_final_text_and_the_next_turn_resumes_its_thread`
4. `a_claude_permission_prompt_yields_to_the_caller_and_the_answer_is_honoured`
5. `cancelling_a_headless_turn_kills_the_cli_and_returns_cancelled`
6. `headless_tool_calls_become_transcript_updates`
7. `a_codex_turns_tool_calls_arrive_at_turn_end`

`packages/tddy-daemon-rpc/tests/list_subagents_acceptance.rs` (extend):

8. `headless_harnesses_are_listed_with_their_transport_and_permission_policy`
9. `a_harness_offered_both_ways_is_listed_once_per_transport`

`packages/tddy-daemon-rpc/tests/session_agent_roster_acceptance.rs` (extend):

10. `a_managed_session_refuses_a_headless_harness_that_cannot_be_confined`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **ACP preferred, both listed.** The operator picks; the row says which transport and which policy.
- **Fixed policies for cursor / codex** because neither CLI can ask mid-turn; the policy is visible,
  never implied.
- **Fake CLIs** keep CI hermetic; a manual smoke against the real CLIs is recorded at green.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-headless-driver-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-agent-backend -p tddy-coding-subagent -p tddy-daemon-rpc`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
