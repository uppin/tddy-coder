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

### Progress (2026-10-25, /green rounds 1–4)

Committed on `feature/coagents/web-approvals` (new commits only, nothing amended). Done: the answer route,
the deferral wire field, non-queueing main-agent answers end to end, the deferred-only user gate,
`deferPermissionToUser` honoured, the owning daemon id threaded into the entry, the answered record
rendered without re-offering options, and the fold's deferred-only `awaiting_user`. The eleven acceptance
tests of this node pass: the Rust targets listed under Verification, and the web spec
`SubagentPermissionDeliveryAcceptance.cy.tsx`. The web spec `SubagentPermissionAnswerAcceptance.cy.tsx`
is red until the fixture edit below is applied.

**Committed in rounds 5–6 (new commits, nothing amended).** Rust: `bd8582958` (D-1, the conversation-status row),
`cabe51772` (D-2, cancel records the pending request, proto field 5, TypeScript regenerated), `d6c0fb838` (D-5 wording),
`dbbc14952` (a cancel test added after green, not red-first, disclosed by its author), `86db79f7d` (TODO markers for
the two live gaps), `d18dd8eb8` (D-3, remote answers forwarded to the owner), `617a52283` (D-6, `github_user` required).
Web: `6c9623f31` (the owner is the roster half). **Held, uncommitted in the worktree:** the cancelled-record wording
(W-2) and the cancel-actor login fix. Both need the decisions in the open items below.

**Verified on HEAD `617a52283` (scoped, all passed):** tddy-coding-subagent: permission_defer 2, permission_yield 7,
coding_agent_session 10, main_agent_answer 2, permission_frames 4, permission_wording 2. tddy-tools: permission_deferral_arg
3, permission_deferral_withdrawal 2, session_agent_conversation_client 18, subagent_cancel_actor 1, subagent_resume_defer 2,
subagent_resume_withdrawal 2. tddy-session-agents: --lib 56, permission_transcript_frames 1, remote_answer_forward 2.
tddy-service: awaiting_user_fold 5, conversation_row_fold 4, subagent_conversation_replay 8. tddy-daemon-rpc:
answer_agent_permission 2, cancel_records_refusal 4, defer_permission_wire 3, main_agent_withdrawal 2, waiting_state_record 2.
tddy-discovery: --lib 97, subagent_runtime_answer_bypass 2. tddy-session-lifecycle: --lib 165.

**Code this node grew, for the [W] sweep (recorded now, not refactored in [G], per the developer's scope change):**

- `packages/tddy-session-agents/src/service.rs`: 1,432 lines, grown by +254 / -39. Oversized. The growth is in
  `answer_agent_permission`, `claim_for_answer`, the resume path's status-row writes, and `cancel_agent_conversation`
  (cancel recording).
- `packages/tddy-discovery/src/subagent_runtime.rs`: 1,167 lines, grown by +82. Oversized. `run_turn` grew with the
  answer bypass.
- `packages/tddy-coding-subagent/src/session.rs`: 654 lines, grown by +145. `take_turn` and `resume` grew (the ordering,
  the deferral wait, the streamed records).
- `packages/tddy-service/src/subagent_conversation_replay.rs`: 397 lines, grown by +184. `entry_status` and the row
  precedence grew.
- `packages/tddy-session-agents/src/conversation_recorder.rs`: 502 lines, grown by +69. `ports.rs` +43,
  `agent_conversations.rs` +52.
- Already oversized, grown only slightly: `packages/tddy-tools/src/server.rs` (3,959 lines, +18) and
  `packages/tddy-discovery/src/subagent.rs` (2,096 lines, +10).

**Status after rounds 5–6 (2026-10-25).** Each open item says whether it is built.

1. **Waiting state: built (D-1), with two open gaps.** The daemon writes one conversation-status row per deferred
   turn: call id `subagent-conversation:<conversation_id>`, tool name `subagent_conversation`. The row is running with
   `awaitingUser: true` when the deferral is accepted, is cleared when the answer is delivered, and is completed at turn
   end. The fold lets a running row decide the entry's status. Once the row is completed, the newest row by time decides.
   Open: the row is durable only. It is not published on the activity hub, so a live subscriber sees it at the next
   snapshot, and the raw `StreamAgentActivity` snapshot returns it as a `subagent_conversation` tool call. Both are
   `TODO(web-approvals)` in `conversation_status.rs`. A refused deferral shows running-with-awaiting briefly, then completed.
2. **Cancel: built, with two open gaps (D-2).** `CancelAgentConversationRequest.cancelled_by` (proto field 5) carries the
   cancelling actor: `main-agent` from `subagent_cancel`, and the signed-in login from the web. For a pending permission
   request, the daemon writes a completed `permission:<requestId>` record with raw output `{answeredBy, cancelled: true}`,
   then interrupts the turn. A reopened dialog shows "Cancelled by <actor>" with no options, and AnswerAgentPermission
   then returns NOT_FOUND, since the conversation is no longer open.
   - **(A) Trust: resolved in round 7; the `main-agent` label resolved in round 8 by option (a), the agent principal
     (developer decision).** The daemon derives the cancelling actor from the authenticated caller, and
     `cancelled_by` is removed from the request (proto field 5 reserved, `772bff4c9`). A caller cannot record another
     login. **Blocked, decision needed:** the `main-agent` label. The daemon cannot tell the agent's own token from the
     user's: `SessionClaims` has no principal, and `TokenKind` has only `Access` and `Refresh`. Option (a): add an
     agent principal claim and mint it for every agent token, local and split. Option (b): attribute every cancel to
     the signed-in login. Recommend (b) for this node.
   - **(B) Web cancel while a deferred request is pending, decision needed.** The committed web code sends no actor, so
     that cancel returns an internal error after the conversation has been cancelled. The fix (send the signed-in login)
     is written but held uncommitted, because it breaks `SessionAgentAttachTabAcceptance` AC10, which mounts with no
     signed-in user. Decision: consent to an AC10 fixture edit, or a different null-login policy.
3. **Remote answers: built (D-3).** A remote conversation's answer is forwarded to its owner through
   `AgentConversationPeers::answer_permission`. The owner's response or refusal is returned as it is.
4. **Owner id: built.** The entry addresses its answer to the roster half (`rosterHalfOf(session)`), which holds the
   conversation, and pairs it with that half's session id. A co-located session uses its own daemon, which the existing
   fixtures express. Committed in `6c9623f31`.
5. **Wording: built (D-5).** The refusal reads "the turn holding it carries on, and its outcome goes to the call that
   deferred the request."
6. **awaitingUser lifetime: decision recorded.** `awaitingUser` stays true from registration until the turn ends,
   including the window between the user's answer and the continuation. The developer accepted this.
7. **github_user: built (D-6).** Required on `AgentSessions`, and its TODO is removed.

**Round 8 (2026-10-25): the agent principal (developer decision, chosen in person).** Committed: `e46a18733` (the
held web work: the web sends no actor on cancel, a cancel with no signed-in user is refused and sends nothing, and a
cancelled request reads "Cancelled by <actor>"), `4b8b7e241` (red), `b85d77c6e` (green).

- `SessionClaims.principal: Option<TokenPrincipal>` (`tddy-github`), `TokenPrincipal::Agent` serialised `"agent"`,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. A user token is byte-for-byte what it was; a token minted
  before the claim existed reads as a user's; a verifier that predates the claim ignores it (no `deny_unknown_fields`),
  so it reads an agent's token as the user's. The signature covers it. An unknown principal value is malformed. No
  deployed token breaks.
- `SessionTokenSigner::mint_agent(user, ttl)`. `mint_agent_session_token` uses it, and every agent credential goes
  through that function: the local coding-agent tool server (`CodingAgentToolHost::tool_server`), the split agent
  (`split_remote_tool_env`), the sandboxed-codebase and project-home agents (`agent_session_token_for`). The local
  CLI's `MintLocalToken` and the room poller's per-poll token stay user tokens.
- `AgentSessions::answerer(token) -> AnsweredBy` (required). The daemon's adapter authenticates with the user resolver
  as everywhere, then reads the principal off the token its own `SessionTokens` verify: `MainAgent` when it verifies
  and carries `Agent`, otherwise `User(login)`. A host built without session tokens mints no agent token, so no caller
  is the agent there. An agent token no trusted key signed is unauthenticated. `cancel_agent_conversation` records
  `answerer(..).name()`.
- Test support: `tddy_session_lifecycle::test_util::signing_test_host` and
  `tddy_daemon_rpc::test_util::signing_test_service_with_coding_agent_launcher` build a host that authenticates real
  tokens, with `SignedTestCredentials { user_token, agent_token }` for `TEST_USER`.
- **The user's answer route refuses the agent's token (developer decision, built).** `AnswerAgentPermission` over an
  agent-principal token is `PERMISSION_DENIED`, naming `subagent_resume` as the route the main agent answers through.
  The daemon's `AgentSessions::github_user` refuses it, and the handler asks for the login right after authentication,
  before routing or forwarding. Pinned by `cancel_actor_principal_acceptance::the_users_answer_route_refuses_the_agents_token`.
- The round-8 test edits below were approved by the developer and applied by the orchestrator in `1ffc992b6`.

**Verified at the end of round 8 (scoped, all passed):** Cypress, one spec at a time: SubagentConversationCancelActor 2,
SubagentPermissionCancelled 3, SubagentPermissionDelivery 6, SubagentPermissionAnswer 4, SubagentPermissionOwner 2,
SessionAgentAttachTab 14, SubagentConversationBubble 5. tddy-github --lib 78; tddy-session-split --lib 37;
tddy-daemon-auth --lib 53; tddy-session-lifecycle --lib 165; `./test -p tddy-session-agents`: every target passes but
`workspace_roster_admission` (3, the pre-existing remote-codebase stub). tddy-daemon-rpc: cancel_actor_principal 4,
cancel_records_refusal 4, answer_agent_permission 2, defer_permission_wire 3, main_agent_withdrawal 2,
waiting_state_record 2, session_agent_conversation 5, coding_subagent 15, in_jail_conversation 1. tddy-tools
subagent_cancel_actor 1. `cargo clippy --all-targets -D warnings`: tddy-daemon-rpc, tddy-session-lifecycle,
tddy-session-agents, tddy-github, tddy-session-split clean.

**Pending existing-test edits after round 8 (the orchestrator applies these; each was checked by compiling and running
a temporary copy, then removed).** These supersede round 7's for the two files named.

- `packages/tddy-daemon-rpc/tests/cancel_records_refusal_acceptance.rs` (4/4 pass with it): L20
  `test_service_with_coding_agent_launcher` becomes `signing_test_service_with_coding_agent_launcher`; L36
  `{TEST_TOKEN, TEST_USER}` becomes `{SignedTestCredentials, TEST_USER}`; after L119 insert
  `    credentials: SignedTestCredentials,`; L154 `let service = test_service_with_coding_agent_launcher(` becomes
  `let (service, credentials) = signing_test_service_with_coding_agent_launcher(`; L172, L183, L198
  `TEST_TOKEN.to_string()` becomes `credentials.user_token.clone()`; after L220 (`        service,`) insert
  `        credentials,`; L234, L267 `TEST_TOKEN.to_string()` becomes `self.credentials.user_token.clone()`; L317 doc becomes
  "Cancel the conversation over `session_token`: the daemon derives the actor from it."; L318 becomes
  `async fn cancel_over(&self, session_token: &str) -> Result<CancelAgentConversationResponse, Status> {` (rustfmt
  splits it); L322 `TEST_TOKEN.to_string()` becomes `session_token.to_string()`; delete L326; L370, L445, L477
  `.cancel("main-agent")` become `.cancel_over(&paused.credentials.agent_token)`; L415 `.cancel(TEST_USER)` becomes
  `.cancel_over(&paused.credentials.user_token)`. The test at L360–404 stays.
- `packages/tddy-tools/tests/subagent_cancel_actor_acceptance.rs` (1/1 passes with it): L1–2 doc becomes "`subagent_cancel`
  reaches the daemon running a remote conversation's loop over the main agent's own session token, which is what that
  daemon derives the cancelling actor from: the token minted for the agent carries the agent principal, so the daemon
  records `main-agent` for a deferred request the cancel closes."; L118 fn becomes
  `subagent_cancel_reaches_the_daemon_over_the_main_agents_token`; L146 comment becomes "Then the tool reports it
  cancelled, and the daemon received it over the main agent's token"; L151 becomes
  `assert_eq!(cancels[0].session_token, an_envelope().session_token);`.
- `AgentSessions::answerer` (required, round 8). Insert after the `github_user` method in every
  `impl AgentSessions for …` fake: `remote_answer_forward_acceptance.rs` after L195 (2/2 pass with it), and, beside the
  round-7 `github_user` insertions, `agent_conversation_system_prompt.rs`, `conversation_transcript_record.rs` (two
  fakes) and `clone_placement_reattach_acceptance.rs`:
  ```
      fn answerer(
          &self,
          _session_token: &str,
      ) -> Result<tddy_discovery::subagent::AnsweredBy, Status> {
          Err(Status::internal(
              "this test cancels no pending request, so it names no answerer",
          ))
      }
  ```

**Code this node grew in round 8, for the [W] sweep:** `packages/tddy-github/src/session_token_v2.rs` 792 → ~900
lines (mostly tests); `packages/tddy-session-lifecycle/src/test_util.rs` +~80 (the signing host).

**Round 7 (2026-10-25), final pending edits.** These supersede the list below where they differ. The five cancel-literal
edits in that list are no longer needed: after the field removal those literals compile as they are.

- `packages/tddy-daemon-rpc/tests/cancel_records_refusal_acceptance.rs`: L317 doc becomes "Cancel the conversation.";
  L318 `async fn cancel(&self, cancelled_by: &str)` becomes `async fn cancel(&self)`; delete L326
  `                cancelled_by: cancelled_by.to_string(),`; L370, L445 and L477 `.cancel("main-agent")` become `.cancel()`;
  L415 `.cancel(TEST_USER)` becomes `.cancel()`; delete the test at L360–404
  (`cancelling_a_deferred_request_as_the_main_agent_records_it_answered_by_the_main_agent`, whose L390 expects
  "main-agent"; L407 covers the same case).
- `packages/tddy-tools/tests/subagent_cancel_actor_acceptance.rs`: delete L151
  `assert_eq!(cancels[0].cancelled_by, "main-agent");`; rename the fn at L118 to
  `subagent_cancel_reaches_the_daemon`; fix the L1–2 doc, which claims it names the actor.
- Session-agents integration fakes (E0046 on the required methods, from D-3 and D-6; these targets do not compile until
  applied): the `answer_permission` and `github_user` insertions listed below are unchanged.

**Pending existing-test edits (the orchestrator applies these; no implementer edits existing tests):**

Rust. Each cancel literal needs one field line, and each fake needs one method. Apply bottom-up within a file.
- `packages/tddy-daemon-rpc/tests/session_agent_conversation_acceptance.rs`: after line 88
  (`conversation_id: conversation_id.to_string(),`, the cancel literal at line 84), insert `cancelled_by: String::new(),`.
  The resume literal at line 474 was already fixed in `efe7639fd`.
- `packages/tddy-daemon/tests/session_agent_remote_acceptance.rs`: after line 1135 (the literal at 1131), insert
  `cancelled_by: String::new(),`.
- `packages/tddy-session-lifecycle/tests/split_session_roster_routing_acceptance.rs`: after line 189 (the literal at
  185), insert `cancelled_by: String::new(),`.
- `packages/tddy-daemon-rpc/tests/coding_subagent_acceptance.rs`: after line 371 (the literal at 367), insert
  `cancelled_by: String::new(),`.
- `packages/tddy-daemon-rpc/tests/in_jail_conversation_acceptance.rs`: after line 137 (the literal at 133), insert
  `cancelled_by: "main-agent".to_string(),`.
- `AgentConversationPeers::answer_permission` (required, D-3). Insert into the fake in each file below, after the given
  line. The body is the same in each:
  ```
      async fn answer_permission(
          &self,
          _session_token: &str,
          _session_id: &str,
          owner: &str,
          _request: &tddy_service::proto::session_agents_svc::AnswerAgentPermissionRequest,
      ) -> Result<tddy_service::proto::session_agents_svc::AnswerAgentPermissionResponse, Status> {
          Err(Status::internal(format!(
              "this test answers no permission, so none is forwarded to '{owner}'"
          )))
      }
  ```
  - `packages/tddy-session-agents/tests/agent_conversation_system_prompt.rs`: after line 322 (the end of `cancel`, in the
    fake starting at 280).
  - `packages/tddy-session-agents/tests/conversation_transcript_record.rs`: after line 329 (the fake starting at 287).
  - `packages/tddy-session-agents/tests/clone_placement_reattach_acceptance.rs`: after line 191 (the fake starting at 139).
- `AgentSessions::github_user` (required, D-6). Insert after the given line, in each fake:
  ```
      fn github_user(&self, _session_token: &str) -> Result<String, Status> {
          Err(Status::internal("this test takes no permission answer, so it names no GitHub user"))
      }
  ```
  - `packages/tddy-session-agents/tests/agent_conversation_system_prompt.rs`: after line 222 (the fake at 175).
  - `packages/tddy-session-agents/tests/conversation_transcript_record.rs`: after line 236 (the fake at 198), and after
    line 553 (the fake at 515).
  - `packages/tddy-session-agents/tests/clone_placement_reattach_acceptance.rs`: after line 135 (the fake at 99).

Web:
- `packages/tddy-web/cypress/component/SubagentPermissionDeliveryAcceptance.cy.tsx` line 242: `"Answered by alice"`
  becomes `"Cancelled by alice"`. This is needed for the held W-2 wording.
- `packages/tddy-web/cypress/component/ActivitiesTailScrollAcceptance.cy.tsx`: line 70 `daemonInstanceId="daemon-a"` becomes
  `rosterHalf={{ sessionId: "s1", daemonInstanceId: "daemon-a" }}`; line 105 becomes
  `rosterHalf={{ sessionId, daemonInstanceId: "daemon-a" }}`. This is tsc only, since `SessionActivitiesPane` now takes
  `rosterHalf`.
- `packages/tddy-web/cypress/component/SessionAgentAttachTabAcceptance.cy.tsx` AC10: the consent decision in item 2(B).

**Not verified at HEAD, blocked by the pending edits above:** `conversation_transcript_record`,
`agent_conversation_system_prompt`, and `clone_placement_reattach_acceptance` (session-agents), and `--all-targets`
clippy on `tddy-daemon-rpc` and `tddy-session-lifecycle`.


**Pre-existing reds outside this node, left untouched:** `transports_acceptance` (4, headless-driver's
TODO stubs), `workspace_roster_admission` (3, remote-codebase's TODO stub), and
`projectHomeStartSpec.test.ts` (3, fixture lacks `clonePlacedAgentIds`).

**Known flake:** `coding_agent_session_acceptance::cancelling_a_running_turn_returns_cancelled` failed once in
a full package run and then passed in every later run (three repeats, plus 15 sequential and 40 parallel runs
in an earlier round). Watch CI.

## Decisions & Trade-offs

- **Explicit defer, not "show to everyone".** Buttons appear only after the main agent defers, which
  keeps the decision the main agent's, as the user chose.
- **First answer wins** rather than locking the main agent out once deferred — a main agent that
  changes its mind is not stuck.
- **Answer route built in this node (developer decision, 2026-10-25).** The conversation's answering
  handle is captured at open and stored beside the conversation (`AgentConversation::Local.answering`),
  so `AnswerAgentPermission` delivers to `ParkedPermissions`. `PermissionAnswering` and `AnsweredBy`
  moved to `tddy-discovery`, and `SubagentSession::permission_answerer` (default `None`) exposes the
  handle through the boxed session. `tddy-coding-subagent` re-exports them, so existing paths hold.
- **Deferral wire field (developer decision).** `ResumeAgentConversationRequest.defer_permission_to_user`
  (proto field 11, `optional string`) is additive and carries the deferral from the tools layer to the
  daemon. #722's message is otherwise unchanged. The regenerated TypeScript carries only that field.
- **A main-agent answer does not queue (developer decision).** The daemon claims the session without
  waiting. Where a deferred turn holds it, the answer is delivered through the handle and the call is
  refused at once with FAILED_PRECONDITION naming the delivery. The tools runtime sends a permission
  answer to the daemon without waiting on its own lock. The PRD's withdrawal holds: the main agent may
  still answer the request it deferred.
- **The daemon accepts a user answer only for a deferred request (developer decision).** The gate is in
  the `AnswerAgentPermission` handler. The session-level `answer_as` stays permissive, because
  `permission_defer_acceptance.rs` answers a non-deferred request as a user and expects `Ok`.
- **`deferPermissionToUser` is advertised and honoured (developer decision).** `subagent_resume` hands
  the request to the user, and a pending turn says `awaitingUser: true`.
- **Only deferred requests fold to `awaiting_user` (developer decision).** A request the main agent
  must answer itself reads in progress. Its transcript entry is `permission:<requestId>`, with
  `deferredToUser` in its input.
- **Kept off-brief changes (developer decision).** The subagent bubble's status renders inside #723's
  bubble, and a second main-agent answer reads "already answered by the main agent". The dead
  `ParkedPermissions::answer()` is removed.
- **Answered records are re-emitted as completed tool calls.** The web projection ignores
  `tool_call_update`, so the answered record is a completed `ToolCallRecord` under the same
  `permission:<requestId>` id, carrying `{answeredBy, optionId}` or `{answeredBy, cancelled: true}`.
  `answeredBy` is the login for a user and `main-agent` for the main agent.

- **Cancelled deferred requests are recorded (developer decision).** `subagent_cancel` carries the cancelling actor
  (`cancelled_by`). The daemon writes a completed `permission:<requestId>` record with `{answeredBy, cancelled: true}`
  before it interrupts the turn. A reopened dialog shows "Cancelled by <actor>" and offers no options.
- **The waiting state is a daemon-written row (developer decision).** One conversation-status row per deferred turn,
  under the stable call id `subagent-conversation:<conversation_id>`. It is running with `awaitingUser` on defer, cleared
  on answer, and completed at turn end. The fold lets a running row decide the entry's status. Once it is completed, the
  newest row by time decides.
- **Remote answers forward to the owner (developer decision).** Through `AgentConversationPeers::answer_permission`, and
  the owner's response or refusal is returned as it is. The local route lookup is one `AnswerRoute` (`answer_route_for`),
  which replaced `answering_for` and `is_open_on`.
- **`github_user` is required on `AgentSessions` (developer decision).** Its default and its TODO are removed.
- **The refusal wording names the holder (developer decision).** "the turn holding it carries on, and its outcome goes
  to the call that deferred the request."
- **Accepted: `awaitingUser` lasts until the turn ends (developer decision).**
- **The daemon derives the cancelling actor from the token (developer decision, rounds 7–8).** `cancelled_by` is gone
  from the request. A token minted for the session's agent carries `TokenPrincipal::Agent` and its cancel is recorded as
  `main-agent`; any other authenticated token is recorded under its login. The claim is optional on the wire, so tokens
  issued before it read as user tokens, and nothing treats an unverifiable token as the agent.

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
- [x] TDD Green — implement with quality code (open gaps kept as `TODO(web-approvals)`: the status row is not published on the activity hub; the peer-forward cross-daemon test)
- [x] Update documentation with progress
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
