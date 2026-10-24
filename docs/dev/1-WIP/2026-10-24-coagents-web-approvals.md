# Changeset: The main agent can hand a subagent's permission request to the user

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — proto change (new RPC, peer-forwarded), a first-answer-wins race between the web and the main agent, Rust + web
**Branch**: `feature/coagents/web-approvals`, base `feature/coagents/headless-driver` (#726) — `#coagents` 8/8
**PR**: [#727](https://github.com/uppin/tddy-coder/pull/727)

## Initial Discovery

[2026-10-24-coagents-web-approvals-initial-discovery.md](./2026-10-24-coagents-web-approvals-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ⚠ DURING — A conversation id is not bound to the session that opened it — [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)

`AnswerAgentPermission` is authorised against the session it names **and** checks the conversation
belongs to it — it must not inherit the host-global lookup's gap, since it is a new write path from
the web.

## Responsibility

- `SessionAgentService.AnswerAgentPermission(AnswerAgentPermissionRequest { session_token, session_id, daemon_instance_id, conversation_id, request_id, option_id | cancelled }) → AnswerAgentPermissionResponse { answered_by }`; peer-forwarded in `PeerRoutedSessionAgents`.
- `PermissionAnswer::DeferToUser { request_id }`; `ParkedPermissions` gains a deferred state and a single first-answer-wins resolution (`AnsweredBy { MainAgent, User(login) }`).
- `tddy-tools`: `subagent_resume` `permission.deferToUser`; pending JSON `awaitingUser`.
- Transcript: a deferred request frame and an answered frame (who, which option) — n5's recording.
- Folded bubble status `awaiting_user` (n5's fold).
- Web: `PermissionRequestEntry` in the conversation view (buttons only when deferred), `useAnswerAgentPermission`; bubble label "waiting for you".

## Boundaries

- No notifications (Telegram, desktop) for a deferred request. No answering from the Agents tab.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n3 `permission-yield` (#722) | `ParkedPermissions`, `PermissionAnswer`, `PermissionRequestView`, resume wire field | adds `DeferToUser` and a second resolver | change how the main agent answers |
| n5 `conversation-bubble` (#723) | conversation transcript + replay axis, `SubagentConversationDialog`, fold statuses | renders the entry and buttons; adds `awaiting_user` | change recording or the replay RPCs |
| n1, n2, n7, n8, n4 | inherited | — | — |

## Draft PR contract

The RPC + messages (regenerated TS), `DeferToUser`, `AnsweredBy`, the web component and hook shells —
bodies `TODO(web-approvals)` — and the failing tests below.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — needs n3's parked-permission behaviour and n5's dialog
**Concurrent with:** n4, n8
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6      n7 → n8

## Affected Packages

- **tddy-service**: `session_agents.proto` (regenerated TS)
- **tddy-coding-subagent**: deferred state, first-answer-wins
- **tddy-session-agents** / **tddy-session-lifecycle**: RPC handler + peer forward
- **tddy-tools**: `deferToUser`, `awaitingUser`
- **tddy-web**: entry, buttons, hook, bubble label

## Related Feature Documentation

- [PRD-2026-10-24-coagents-web-approvals.md](../../ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md)

## Summary

The main agent can defer a subagent's permission request to the user, who answers it from the
conversation view.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

After n3/n5: a parked request is resolved only by `TurnRequest.permission_answer` from the caller; the
conversation view renders a pending request read-only.

### State B (Target)

```
main agent: subagent_resume{permission:{requestId, deferToUser}} → {responseId, pending, awaitingUser}
web: AnswerAgentPermission{conversation, requestId, optionId} → ParkedPermissions.resolve(User) → agent continues
main agent: subagent_await{responseId} → continuation
```

## Testing Plan

**Level:** integration for the race and the RPC (stub coding agent with a permission scenario);
Cypress component for the entry (`mountWithRpc` + in-memory `SessionAgentService` backend).

## Acceptance Tests

`packages/tddy-coding-subagent/tests/permission_defer_acceptance.rs` (stub agent paused on a request; needs #721 and #722 green):

1. `a_deferred_request_waits_for_the_users_answer_and_the_turn_then_continues` — `TurnRequest::deferring_permission_to_user` holds the turn; `PermissionAnswering::answer_as(.., User("alice"))` continues it; the agent records `selected:allow-once`
2. `the_first_answer_wins_and_the_second_is_refused_naming_who_answered`

`packages/tddy-tools/tests/permission_deferral_arg_acceptance.rs` — `subagent_resume`'s `deferPermissionToUser`:

3. `a_request_id_is_deferred_to_the_user` · 4. `no_deferral_is_none` · 5. `deferring_and_answering_at_once_is_refused_naming_both`

`packages/tddy-daemon-rpc/tests/answer_agent_permission_acceptance.rs`:

6. `answer_agent_permission_is_refused_for_an_unauthenticated_caller` — `UNAUTHENTICATED`
7. `answer_agent_permission_is_refused_for_a_conversation_the_session_does_not_hold` — `NOT_FOUND` naming it

`packages/tddy-web/cypress/component/SubagentPermissionAnswerAcceptance.cy.tsx` (over `aConversationReplayBackend` + a recorded `AnswerAgentPermission`) — fail first at #723's missing bubble, then on this node's entry:

8. `a deferred request shows one button per option` — `permission-option-<optionId>`
9. `a request not deferred to the user shows no buttons`
10. `choosing an option sends it and the entry records the choice` — `{conversationId, requestId, optionId}`; `permission-answered` names `alice`
11. `the bubble reads waiting for you while a request is deferred`

**Changed from the plan:** this node owns no symbol of #722's: the deferral is `TurnRequest::deferring_permission_to_user` (a new builder, not a `PermissionAnswer` variant), the second resolver is the new `PermissionAnswering` trait implemented for `ParkedPermissions` (plus `AnsweredBy`), and the MCP argument is a separate `deferPermissionToUser` property rather than a field inside #722's `permission`. The transcript carries a pending request as a `tool_call` with id `permission:<requestId>` whose detail names the options and `deferredToUser` — the conversation view reads it through the existing detail lookup. The peer-forward of `AnswerAgentPermission` is published as a local delegation (`TODO(web-approvals)`); its cross-daemon test needs two daemons and is green's. Non-owner refusal is pinned as "unauthenticated" because the test daemon authenticates one user; green adds the second-user case if the host's owner check differs from token resolution.

**Baseline (2026-10-24, isolated `CARGO_TARGET_DIR`):** `cargo check --all-targets` clean across service, session-agents, session-lifecycle, daemon-rpc, daemon, tools, coder, session-split, discovery, coding-subagent; scoped clippy clean.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Explicit defer, not "show to everyone".** Buttons appear only after the main agent defers, which
  keeps the decision the main agent's, as the user chose.
- **First answer wins** rather than locking the main agent out once deferred — a main agent that
  changes its mind is not stuck.

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

- [x] Record initial discovery (`2026-10-24-coagents-web-approvals-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-coding-subagent -p tddy-session-agents -p tddy-daemon-rpc -p tddy-tools-cli`; the new Cypress spec) — verify 100% pass
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
