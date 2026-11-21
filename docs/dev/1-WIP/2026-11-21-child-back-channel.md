# Changeset: An orchestrator's child is told how to report back, and a target child's report reaches it

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — guidance rendered per (agent, placement), a new hook-log event parsed by a stack-shared follower, a native-peer naming risk to verify
**PR**: [#980](https://github.com/uppin/tddy-coder/pull/980)
**Stack**: `#orchestra` 2/9 — branch `feature/orchestra/child-back-channel`, base `feature/orchestra/path-reach`

## Initial Discovery

[2026-11-21-child-back-channel-initial-discovery.md](./2026-11-21-child-back-channel-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: child guidance (orchestrator id, back channel by placement); `message` line handled by the remote hook follower, in a new module
- **tddy-core**: `remote_message_command` — the `printf` line a target child appends to `.tddy/hooks.log`
- **tddy-session-activity**: a target child's message lands as the same `message` record `PostSessionMessage` writes
- **tddy-coding-agent-specs**: how an agent is addressed natively (peer name), declared per agent

## Related Feature Documentation

- PRD: [PRD-2026-11-21-child-back-channel.md](../../ft/daemon/1-WIP/PRD-2026-11-21-child-back-channel.md)

## Summary

Every session started by an orchestrator gets launch guidance that names its orchestrator and **the one back channel it should use to report progress and status**: a Claude child on the orchestrator's host messages it natively as a Claude peer; any other local child runs `tddy-tools send`; a child on a Coder or SSH target appends a `message` line to its worktree's `.tddy/hooks.log`, which the daemon already tails and now turns into a session-message record for the orchestrator.

## Responsibility

- Back-channel guidance rendering and its choice of channel per placement and agent.
- Pinned native peer names for Claude children and their record on the session.
- The `message` hook-log event: writer command (`tddy-core`), parser and follower mapping (`tddy-agent-launch`).

## Boundaries

- Not the orchestrator's side of delivery (n7) and not its prompt (n9).
- No cross-host `PostSessionMessage` forwarding.
- Does not split `remote_hook_follower.rs` — reserved for `#target-parity`'s sweep; new code goes in its own module.

## Dependencies

None inside `#orchestra`. It builds on its base, `#target-parity` 7/7 (#975), and on what is on master.

## Draft PR contract

The first push of this PR, before its implementation: `BackChannel` (enum Native/ToolsSend/HookLog) + `back_channel_for(...)`, `remote_message_command`, `HookEvent::Message`, `native_peer_name_of(session_id)`; failing tests below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — builds on #975's follower only
**Concurrent with:** n1, n3, n4, n5
**Blocks:** n7 (routes to the peer name and channel this node records), n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### ⚠ DURING — [oversized-file-remote-hook-follower.md](../../../packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-hook-follower.md)

Deferred to `#target-parity`'s sweep (stack-shared). The `message` mapping goes in its own module; the follower only dispatches to it.

### ⚠ DURING — [2026-11-19-coder-remote-agent-local-paths.md](../todo/2026-11-19-coder-remote-agent-local-paths.md)

L3/L4 (notify wiring on a target) is the same gap from the other side. This node gives targets a report line, not `notify`; L3/L4 stay open.

### ✅ Resolved at contract time — native peer naming

Claude Code 2.1.323 takes `-n, --name <name>`, the name same-host peer messaging addresses a session by. The name is derived from the session id (`native_peer_name_of`), so nothing new is recorded on the session.

## Scope

- [ ] Back-channel guidance for orchestrator children (local Claude, local other, target)
- [ ] Pinned Claude peer name recorded and listed
- [ ] `message` hook-log line → orchestrator `message` record
- [ ] Feature docs `project-concept.md` § Session messages and `claude-cli-session.md` § On a target session updated through the wrap

## Technical Changes

### State A (Current)

`agent_notify_guidance.md` only; `HookLine::parse` (`remote_hook_follower.rs:85`) refuses unknown events; `PostSessionMessage` same-host via `tddy-tools`.

### State B (Target)

Each child knows its orchestrator and channel; target reports arrive as message records.

## Acceptance Tests

`packages/tddy-agent-launch/tests/child_back_channel_acceptance.rs`:
- [ ] `a_local_codex_child_of_an_orchestrator_reports_with_tddy_tools_send`
- [ ] `a_local_claude_child_of_a_claude_orchestrator_reports_natively_to_its_peer_name`
- [ ] `a_claude_child_on_a_target_reports_through_its_hook_log_not_natively`
- [ ] `every_channels_guidance_asks_for_periodic_progress_and_status_reports`
- [ ] `a_message_line_is_a_hook_the_follower_accepts`
- [ ] `a_followed_message_line_is_posted_to_the_orchestrator`
- [ ] `a_message_line_without_text_is_not_posted`
- [ ] `a_native_peer_name_is_stable_and_names_the_session`

`packages/tddy-session-lifecycle/tests/child_back_channel_launch_acceptance.rs` (real `StartSession`, stub `claude` echoing its argv; needs a built `target/debug/tddy-tools`, as `./test` provides):
- [ ] `an_orchestrator_is_launched_under_its_native_peer_name`
- [ ] `a_claude_child_is_launched_under_its_peer_name_and_told_to_report_to_its_orchestrator`
- [x] `a_session_without_an_orchestrator_is_launched_unnamed_and_told_of_none` — regression guard

`packages/tddy-coding-agent-specs/tests/spec_peer_name_acceptance.rs`:
- [ ] `claude_declares_its_peer_name_flag`
- [x] `an_agent_without_native_peer_messaging_declares_no_peer_name` — regression guard

The surface they specify (bodies `TODO(child-back-channel)`): `tddy_core::REMOTE_MESSAGE_EVENT`, `coding_agent.peer_name` (`PeerNameSlot`, `CodingAgentSpec::peer_name`), `tddy_agent_launch::back_channel::{BackChannel, BackChannelInputs, ChildPlacement, RemoteMessage, back_channel_for, back_channel_guidance, native_peer_name_of}`, `HookReportDelivery::post_message_to_orchestrator` (default refuses; the session host's `ActivityHookDelivery` overrides it in the implementation).

Verified: Claude Code 2.1.323 has `-n, --name <name>`, the name same-host peer messaging addresses a session by — the planning-time risk is resolved, and the native route stays.

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- Remote reports ride `hooks.log` as a `message` event, not a separate outbox file (developer, 2026-11-21).
- Native Claude↔Claude messaging is preferred where both are on one host (developer, 2026-11-21).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-child-back-channel-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
