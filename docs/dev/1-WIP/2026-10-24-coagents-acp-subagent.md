# Changeset: A coding agent as a session subagent, over ACP

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto + wire-record change, a new crate, a spawned-as-user process seam, four crates and the web, managed confinement per harness is partly unsettled (see Decisions)
**Branch**: `feature/coagents/acp-subagent`, base `feature/coagents/acp-driver` (#720) — `#coagents` 2/8
**PR**: _assigned at wave 1_

## Initial Discovery

[2026-10-24-coagents-acp-subagent-initial-discovery.md](./2026-10-24-coagents-acp-subagent-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ⚠ DURING — Drain `tddy-discovery` into purpose-named crates — [`2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md`](../todo/2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md)

New code does **not** go into `tddy-discovery`. The coding-agent session lives in a new
purpose-named crate, `tddy-coding-subagent`, which depends on `tddy-discovery` only for the
`SubagentSession` trait and outcome types (they move to `tddy-subagent` when the drain runs; this
crate follows them).

### ⚠ DURING — A conversation id is not bound to the session that opened it — [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)

A coding-agent conversation lives in the same host-global map. No worse than a `model` conversation;
the stake is higher because the agent can write. Recorded, not fixed here.

### ⚠ DURING — A subagent turn has no wall-clock deadline — [`2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md`](../todo/2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md)

A coding agent stalls the same way; `subagent_cancel` (now real `session/cancel`) is the way out.

### ℹ ANSWERED — Daemon-run conversations still write the session worktree — [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)

For this node that is the **intended** default ("same worktree as the main agent"). n7
`clone-placement` claims the entry.

### ℹ ANSWERED — Tool-session provider picker needs harness metadata — [`2026-10-22-tool-session-provider-picker-needs-harness-metadata.md`](../todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md)

`SubagentInfo.harness` is that metadata for the subagent catalog; `AgentInfo` (the tool-session
picker) is not changed here.

## Responsibility

- **Kind on the wire and at rest.** A shared `AgentKind { MODEL = 0, CODING_AGENT = 1 }` and an
  optional `harness` on `SubagentInfo` and `SessionAgentEntry`; `kind` + `harness` on
  `tddy_changeset::SessionAgentRecord` (serde default `Model`, `harness: None`).
- **The coding-agent catalog.** `CodingAgentDef { name, label, harness, model, provider_account }`
  resolved from ACP harness profiles and ACP `allowed_agents` ids; rows added to `ListSubagents`
  after the `model` rows.
- **Admission.** Attaching a `coding_agent` row; refusing, for a managed session, a harness whose
  native tools cannot be withdrawn (`HarnessConfinement::cannot_withdraw`).
- **The session.** `tddy_coding_subagent::CodingAgentSession: SubagentSession`, driving n1's
  `AcpDriver`: turn → `PromptOutcome`, ACP tool calls → `MessageDescriptor`s, loop-only request
  options refused (`SubagentError::UnsupportedByCodingAgent { option }`), permission requests denied
  through a `DenyAndRecord` `PermissionHandler`.
- **The launch.** `CodingAgentLauncher` — the port `tddy-session-agents` calls to start the agent
  process for a conversation: as the session's OS user, the provider account's route applied, cwd on
  the session worktree; in a managed session native tools withdrawn and `tddy-tools --mcp` handed to
  the agent as an MCP server pointed at the session's exec tools. Its daemon adapter lives in
  `tddy-session-lifecycle`.
- **Open / turn / end.** `open_local_agent_session` builds a `CodingAgentSession` for a
  `coding_agent` record; cancel, end and detach stop the process.
- **Web.** The kind badge in `CreateSessionAgentPickerSection`, `AgentPicker` and `RosterAgentRow`.

## Boundaries

- No permission yield to the main agent — n3 `permission-yield` replaces `DenyAndRecord`.
- No transcript persistence, no activity bubble — n5 `conversation-bubble`.
- No separate clone placement — n7 `clone-placement`. Conversations run on the session worktree.
- No remote coding agents (`name@other-daemon` with `kind: coding_agent` is refused at attach) — n8 `remote-codebase`.
- No headless `-p` harnesses (`claude`, `cursor`, `codex`) — n4 `headless-driver`; `ListSubagents` does not list them yet.
- No `replaces` for coding agents (always empty).
- `model` entries and the in-jail loop are untouched.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `AcpDriver::{spawn, open_session, prompt, cancel}`, `OpenSession { cwd, resume, model, mcp_servers }`, `AcpTurnEnd`, `AcpUpdate`, `PermissionHandler`, `McpServerSpec` in `tddy-agent-backend` | `CodingAgentSession` holds one `AcpDriver` per conversation; installs `DenyAndRecord`; maps `AcpUpdate`s to descriptors | change the driver, add a transport, or touch the three workflow backends |

## Draft PR contract

The first push after this plan publishes:

- Proto fields (`AgentKind`, `harness`) with regenerated Rust/TS.
- `SessionAgentRecord.kind` / `.harness`.
- `packages/tddy-coding-subagent` crate with `CodingAgentDef`, `CodingAgentSession`, `CodingAgentLauncher`,
  `HarnessConfinement`, `DenyAndRecord` — bodies `// TODO(acp-subagent): implement`.
- `SubagentError::UnsupportedByCodingAgent`.
- The failing tests below.

Dependents compile against `AgentKind`, `CodingAgentSession`, `CodingAgentLauncher`, `CodingAgentDef`.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — needs n1's `AcpDriver` *behaviour* (its tests drive a real stub turn through it)
**Concurrent with:** none
**Blocks:** n3 `permission-yield`, n5 `conversation-bubble`, n7 `clone-placement`, n8 `remote-codebase`

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-service**: `proto/catalog.proto`, `proto/session_agents.proto` (+ the shared enum) — regenerated in `tddy-web` and `tddy-rust-typescript-tests`
- **tddy-changeset**: `SessionAgentRecord.kind` / `.harness`
- **tddy-coding-subagent** (new): [README.md](../../packages/tddy-coding-subagent/README.md) — the session, catalog, launcher port, confinement table
- **tddy-discovery**: `SubagentError::UnsupportedByCodingAgent` only
- **tddy-session-agents**: `open_local_agent_session` branch on kind; attach admission; catalog rows
- **tddy-session-lifecycle**: `CodingAgentLauncher` adapter (spawn as the session's OS user, route, MCP config)
- **tddy-daemon-rpc**: `ListSubagents` rows
- **tddy-web**: kind badge in two pickers and the Agents tab

## Related Feature Documentation

- [PRD-2026-10-24-coagents-acp-subagent.md](../../ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md)
- [session-agent-roster.md](../../ft/daemon/session-agent-roster.md), [harness-profiles.md](../../ft/daemon/harness-profiles.md), [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)

## Summary

A roster entry can be an ACP coding agent. The daemon starts it as the session's user on the
session's worktree, under the main agent's confinement, and the existing `subagent_*` tools talk to it.

## Background

See the PRD.

## Scope

- [ ] **Implementation**: kind on the wire and at rest; catalog rows; admission; `CodingAgentSession`; launcher adapter; web badge
- [ ] **Testing**: acceptance tests below
- [ ] **Package Documentation**: `tddy-coding-subagent` README; roster doc § kinds
- [ ] **Code Quality**: no growth in over-budget files beyond wiring

## Technical Changes

### State A (Current)

- `tddy-tools/src/server.rs:1763-1853` `subagent_new_session`: `roster.local_def_for(&entry)` (seed def present) → in-process `SpecializedSubagentSession`; else `open_remote_agent_session` → daemon `OpenAgentConversation`.
- `tddy-session-agents/src/service.rs:783-884` `OpenAgentConversation` → `open_owned` → `open_local` (record on this daemon) → `peers.open`.
- `svc_start_hosted_agent_clone.rs:153-191` `open_local_agent_session` always builds `SpecializedSubagentSession` from `agent_def_for_spawn`; `local_agent_codebase_access` (`:269-334`) runs exec tools on the session worktree as the daemon's identity.
- `service.rs:551-606` attach: `catalog.record_for` → `admission.admit` → `rosters.attach`; `roster_record_for` resolves only `resolvable_agent_defs` (`svc_turn_end_reporter.rs:52-74`).
- `tddy-daemon-rpc/src/catalog/ports.rs:167-185` `ListSubagents` = `resolvable_agent_defs` only; `agent_def.rs:221-231` skips `harness:` documents; `docs/ft/daemon/harness-profiles.md`: profiles "cannot be attached to a session's roster".
- `catalog.proto:73-91` `SubagentInfo` fields 1-7; `session_agents.proto:309-347` `SessionAgentEntry` 1-12; `SessionAgentRecord` (`tddy-changeset/src/session_agent.rs`) has no kind.
- Coding-agent processes are spawned elsewhere as the OS user via `spawn_as_user` / the spawn worker (`tddy-daemon-kernel/src/spawn_as_user.rs:194-225`, `tddy-agent-launch/src/tool_session_spawn.rs:241-262`); routes are resolved for tool sessions by `tddy-agent-launch` (`harness_profile_request.rs`, `provider-route.json`).
- Web: `useAvailableAgents.ts:22-31` `AvailableAgent` (no kind); `CreateSessionAgentPickerSection.tsx:16-59`; `AgentPicker.tsx:44-51`; `SessionAgentTree.tsx:253-300` `RosterAgentRow`.

### State B (Target)

```
subagent_new_session{agent:"codex@A"}  (no seed def → daemon, unchanged route)
   └─► daemon A OpenAgentConversation → open_local(record kind=coding_agent)
          └─► CodingAgentSession::open(def, launcher, cwd/session worktree, managed?)
                 └─► CodingAgentLauncher (spawn as session OS user, route, MCP: tddy-tools --mcp)
                        └─► AcpDriver (n1) ── ACP stdio ── codex-acp / claude-agent-acp / opencode acp
   subagent_prompt → take_turn → AcpDriver::prompt → PromptOutcome{stopReason, content, usage, messages}
```

### Delta

See *Responsibility*, per package as listed in *Affected Packages*.

## Implementation Milestones

- [ ] proto + record kind, back-compat load
- [ ] `CodingAgentDef` resolution and `ListSubagents` rows
- [ ] attach admission incl. managed confinement refusal
- [ ] `CodingAgentSession` turn / descriptors / refusals / deny-and-record / cancel / drop
- [ ] `CodingAgentLauncher` daemon adapter (user, route, cwd, MCP)
- [ ] web badge

## Testing Plan

**Level:** integration for the session (real `tddy-acp-stub` child through n1's driver) and the
daemon RPCs (the `tddy-daemon-rpc` acceptance harness already used for `ListSubagents` and
conversations, with the launcher port injected so the spawn request — user, cwd, MCP config — is
observable without root); Cypress component tests for the badge (`mountWithRpc` + in-memory backend).

## Acceptance Tests

`packages/tddy-coding-subagent/tests/coding_agent_session_acceptance.rs` (`#[serial]`, stub binary):

1. `a_prompt_turn_returns_the_coding_agents_text_as_end_turn`
2. `each_tool_call_of_the_turn_is_one_message_descriptor`
3. `a_loop_only_option_is_refused_naming_it` — `yieldConditions`, `fromMessageId`, `correction`, `replacement`, `maxTurns` each → `UnsupportedByCodingAgent { option }`
4. `a_permission_request_is_denied_and_recorded_in_the_turns_messages`
5. `cancelling_a_running_turn_returns_cancelled`
6. `dropping_the_session_stops_the_agent_process`

`packages/tddy-daemon-rpc/tests/list_subagents_acceptance.rs` (extend):

7. `an_acp_harness_profile_is_listed_as_a_coding_agent_with_its_harness`
8. `an_acp_allowed_agent_is_listed_as_a_coding_agent_with_its_default_model`
9. `a_headless_harness_profile_is_not_listed_yet`

`packages/tddy-daemon-rpc/tests/session_agent_roster_acceptance.rs` (extend):

10. `attaching_a_coding_agent_streams_a_coding_agent_entry`
11. `a_managed_session_refuses_a_harness_that_cannot_withdraw_its_native_tools`
12. `a_coding_agent_owned_by_another_daemon_is_refused_at_attach`

`packages/tddy-daemon-rpc/tests/session_agent_conversation_acceptance.rs` (extend):

13. `opening_a_coding_agent_conversation_launches_it_as_the_session_user_on_the_session_worktree`
14. `a_managed_session_launches_the_coding_agent_with_only_tddy_tools_exec_tools`
15. `a_long_coding_agent_turn_is_awaitable` — prompt over the grace → pending → await → outcome
16. `ending_a_coding_agent_conversation_stops_its_process`

`packages/tddy-changeset/src/session_agent.rs` (unit):

17. `a_record_written_before_kinds_loads_as_a_model_entry`

`packages/tddy-web/cypress/component/CreateSessionAgentPicker.cy.tsx`, `SessionAgentTreeAcceptance.cy.tsx` (extend):

18. `a coding agent row shows its coding-agent badge and harness`
19. `a roster row for a coding agent shows its badge`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Score 5 on purpose.** The boundary was considered: splitting the web badge out would leave a node
  whose agents are invisible in the picker, and splitting the launcher out would leave a session with
  no process. Kept as one node; the badge is small.
- **Confinement per harness is partly unsettled.** Withdrawing native tools is harness-specific:
  `claude-agent-acp` takes `disallowedTools` through `session/new` `_meta`; `opencode` through its
  config file's tool permissions; `codex-acp` has no tool withdrawal, only sandbox modes. The table
  (`HarnessConfinement`) starts with what the red phase can prove against each real adapter's
  documentation; anything unproven is `cannot_withdraw`, which refuses it in managed sessions (never a
  silent unconfined launch). Revisit at 6e.
- **Deny, not allow, until n3.** An auto-approving subagent on the main agent's worktree is the
  behaviour this stack exists to avoid.
- **Process per conversation.** One ACP child per conversation keeps cancel and kill trivially scoped;
  a pooled agent with several ACP sessions is a later optimisation.

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

- [x] Record initial discovery (`2026-10-24-coagents-acp-subagent-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p tddy-coding-subagent -p tddy-session-agents -p tddy-daemon-rpc -p tddy-changeset`; the two Cypress specs) — verify 100% pass
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
