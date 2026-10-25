# Changeset: Coding subagents on the codebase host, in remote-codebase mode

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — four crates, relay forwarding (a cross-daemon RPC seam), a roster on a new session type
**Branch**: `feature/coagents/remote-codebase`, base `feature/coagents/clone-placement` (#724) — `#coagents` 6/8
**PR**: [#725](https://github.com/uppin/tddy-coder/pull/725)

## Initial Discovery

[2026-10-24-coagents-remote-codebase-initial-discovery.md](./2026-10-24-coagents-remote-codebase-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ⚠ DURING — `tddy-coder --remote` bootstrap is unimplemented — `docs/dev/TODO.md` (remote-codebase-mode criteria 23–28)

This node targets the implemented path (relay daemon + `tddy-tools remote start-session` +
`tddy-tools --mcp` with `TDDY_REMOTE_*`). It does not implement the `tddy-coder --remote` bootstrap.

### ⚠ DURING — The ConversationWorktree call over HTTP is unverified — [`2026-10-01-conversation-worktree-over-http-is-unverified.md`](../todo/2026-10-01-conversation-worktree-over-http-is-unverified.md)

`clone` placement through the relay uses that call's HTTP arm. Test 6 exercises it; if it fails, the
entry gets fixed here rather than worked around.

## Responsibility

- `remote start-session --agent <name@daemon>` (repeatable) → `StartSessionRequest.specialized_agents` on a `workspace` session.
- Workspace-session roster admission: `coding_agent` entries owned by the codebase daemon only; others refused naming the entry.
- Relay: `tddy-daemon --relay` forwards `session_agents.SessionAgentService` (stream + unary) to the addressed peer.
- `tddy-session-tool-client`: the relay (`DaemonHttp` to the relay) transport is accepted by `connect_facilitating_daemon` for the roster stream and conversation RPCs when the relay is the endpoint.
- `tddy-tools`: subagent tools advertised in remote-codebase mode once the workspace roster is non-empty.
- Remote placement (developer definition, hand-off cited under Decisions): a subagent attached in remote-codebase mode runs on the codebase host's daemon, on a clone that daemon owns (`open_owned`), with results reported back over the hop. **Blocked — the definition does not reconcile with `open_owned`, the roster's no-clone rule or the workspace admission rule; not implemented.**

## Boundaries

- No `tddy-coder --remote` bootstrap. No `model` agents on workspace sessions. No web UI for remote-codebase sessions (there is none today).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n2 `acp-subagent` (#721) | coding-agent catalog, admission, `CodingAgentSession`, `CodingAgentLauncher` | runs them for a workspace session on the codebase daemon | change kinds, launcher or session |
| n7 `clone-placement` (#724) | `placement` on attach / entry | honoured for workspace sessions | change placement semantics |
| n1, n3, n5 | inherited | permissions and the bubble ride the same RPCs | — |

## Draft PR contract

The `--agent` flag, the workspace admission rule's signature, the relay forward entry, the transport
acceptance — bodies `TODO(remote-codebase)` — and the failing tests below.

**Remote placement — no failing tests written.** The definition is blocked on a developer decision
(Decisions: *Remote placement is blocked*). The surfaces it names are `open_owned`
(`packages/tddy-session-agents/src/ports.rs:158`, called at `packages/tddy-session-agents/src/service.rs:857`)
and the workspace admission (`packages/tddy-session-agents/src/workspace_roster.rs:11`). No surface is
changed for it, and no test is proposed until the decision is made.

## Green wave

**Wave:** 4 of 4 (moved from 3 at the contract: a real edge n7 → n8)
**Greenable independently:** no — needs n2's coding-agent conversation behaviour and n7's placement behaviour
**Concurrent with:** n4, n6
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6      n7 → n8

## Affected Packages

- **tddy-tools**: `remote_cli.rs` `--agent`; advertisement in remote mode
- **tddy-session-tool-client**: relay transport for conversation RPCs
- **tddy-session-agents**: workspace-session admission
- **tddy-daemon** / **tddy-session-lifecycle**: relay forwarding of `SessionAgentService`

## Related Feature Documentation

- [PRD-2026-10-24-coagents-remote-codebase.md](../../ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md)

## Summary

Coding subagents for remote-codebase sessions, launched by and on the codebase daemon, reached through
the relay.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `docs/ft/daemon/session-agent-roster.md` non-goal: "Rosters on `workspace` sessions."
- `tddy-discovery/src/roster/link.rs:85-88` `connect_facilitating_daemon` refuses `DaemonHttp` for roster / conversation RPCs.
- `tddy-session-tool-client/src/lib.rs:236-301` transport detection; remote-codebase mode uses `TDDY_REMOTE_SESSION_ID` + `TDDY_REMOTE_DAEMON_URL` (the relay).
- `tddy-tools/src/remote_cli.rs:20-41` `RemoteSubcommand { ListTools, StartSession, ConnectSession, SyncContext }`.
- Relay forwards `ExecuteTool` / `ListExecTools` to one named peer (remote-codebase-mode criteria 12–14).

### State B (Target)

```
laptop: main agent ─MCP─ tddy-tools --mcp ─HTTP─ relay ─LiveKit─ codebase daemon
                                                     SessionAgentService ─► workspace roster ─► CodingAgentSession
                                                                                           (workspace worktree, session user)
```

## Testing Plan

**Level:** integration — two in-process daemons (relay + codebase) in the existing
`tddy-daemon-rpc` / LiveKit testkit pattern, a stub coding agent on the codebase side.

## Acceptance Tests

`packages/tddy-session-tool-client/tests/daemon_http_rpc_acceptance.rs` — `daemon_http_rpc::DaemonHttpRpcClient` against a raw TCP daemon stub (fail at its `TODO(remote-codebase)`):

1. `a_unary_call_posts_the_request_bytes_to_the_service_method_and_returns_the_answer` — `POST /rpc/<service>/<method>`, `application/proto`
2. `a_server_stream_yields_each_enveloped_message_until_end_of_stream` — `application/connect+proto` envelopes, end-of-stream flag 2
3. `a_connect_error_answer_is_the_status_it_names` — `{"code":"not_found"}` → `NOT_FOUND`
4. `an_error_at_the_end_of_a_stream_is_delivered_as_its_last_item`

`packages/tddy-tools/tests/relay_conversation_link_acceptance.rs` (`#[serial]`, remote-codebase env):

5. `the_relay_transport_carries_the_conversation_rpcs` — `AgentConversationLink::connect()` succeeds over `DaemonHttp` (today: refused)

`packages/tddy-session-agents/tests/workspace_roster_admission.rs` — `workspace_roster::workspace_roster_admits`:

6. `a_coding_agent_of_the_codebase_daemon_is_admitted`
7. `a_model_agent_is_refused_naming_it`
8. `a_coding_agent_of_another_daemon_is_refused_naming_it` — also the case #721 moved here ("a coding agent owned by another daemon is refused")

`packages/tddy-tools-cli/tests/remote_cli_subcommand_acceptance.rs` (extended):

9. `remote_start_session_passes_each_agent` — `specializedAgents` on the workspace `StartSession`

**Changed from the plan:** the relay needs no new forward — it is an ordinary daemon whose `PeerRoutedSessionAgents` already forwards conversation RPCs by `daemon_instance_id`; what was missing is a client that can speak to it over HTTP (1–5). The two-daemon end-to-end suite (relay + codebase daemon over LiveKit, plan tests 3, 4, 6) is left to green: it needs the LiveKit testkit and every predecessor green, and green writes it in `tddy-daemon-rpc/tests/remote_codebase_subagent_acceptance.rs` once 1–9 pass. Plan test 8 (tools advertised in remote mode) follows from 5 — advertisement already keys on a non-empty roster stream, which the link now opens.

**Baseline (2026-10-24, isolated `CARGO_TARGET_DIR`):** scoped clippy clean for `tddy-session-tool-client tddy-tools tddy-session-agents`; the 7 existing `remote` CLI tests pass.

## Technical Debt & Production Readiness

- **`remote` requests are JSON, which the relay cannot decode (blocks `--agent` end to end).**
  `connect_post` (`packages/tddy-tools/src/remote_cli.rs`) posts `application/json`.
  `tddy-connectrpc` hands a unary body to the bridge without transcoding
  (`packages/tddy-connectrpc/src/router.rs:96`), and generated handlers decode it as protobuf
  (`packages/tddy-codegen/src/generator.rs:293`). A JSON body is therefore not a request. Test 9 pins
  the JSON body (`specializedAgents`), so the fix is a developer decision: move `remote` to protobuf
  (test 9's parse changes), or give the router JSON transcoding (a new dependency or a descriptor
  transcoder). Every `tddy-tools remote` subcommand and the JSON arms of
  `dispatch_request_via_daemon_http` and `ask_daemon_over_http`
  (`packages/tddy-session-tool-client/src/conversation.rs:245`) share this. Not checked against a live
  daemon.
- **`ConversationWorktree` over HTTP is still unverified** (`docs/dev/todo/2026-10-01-conversation-worktree-over-http-is-unverified.md`).
  No red test in this node sends it. Prerequisite text "Test 6 exercises it" is stale: test 6 is roster
  admission. The first test that reaches the clone call over the relay is the two-daemon suite below,
  and it inherits the JSON defect above.
- **Daemon-side workspace seeding does not admit.** `workspace_roster_admits` has no production caller.
  A workspace start seeds through `seed_and_start_workspace_session`
  (`packages/tddy-agent-launch/src/workspace_branch_start.rs:36-39`) → `seeded_roster_records_placed`
  (`packages/tddy-session-agents/src/agent_placement.rs:50`) → `seeded_roster_records`
  (`packages/tddy-session-agents/src/svc_resolve_listed_worktree.rs:132`), which resolves any reference.
  A model agent or another daemon's coding agent is seeded today. No red test pins it (test 9 is the CLI
  body only), so it is left for the developer.
- **Two-daemon suite not written** (`packages/tddy-daemon-rpc/tests/remote_codebase_subagent_acceptance.rs`).
  Not attempted in this green pass. It needs a workspace project and repo fixture (defined only inside
  `packages/tddy-session-lifecycle/tests/workspace_session_acceptance.rs`) and a coding-agent profile on
  the codebase daemon, plus the `tddy-acp-stub` binary (now built). Its refusal cases depend on the
  seeding gap above.
- **Subagent tools over the relay are not exercised by a red test.** Advertisement keys on
  `has_an_agent_to_address()` (`packages/tddy-tools/src/server.rs:404`). The roster-before-tool-list
  test uses `TDDY_REMOTE_DAEMON_SOCKET` (UDS), not the HTTP relay.

## Decisions & Trade-offs

- **Remote-codebase mode as built, not as specified.** Criteria 23–28 (`tddy-coder --remote`) are
  unimplemented; the user-facing opt-in is therefore `tddy-tools remote start-session --agent`.
  Wiring `tddy-coder --remote` remains its existing TODO.
- **Coding agents only, local owner only.** A `model` agent gains nothing from the codebase host, and
  a third daemon would be multi-hop (roster non-goal).
- **The roster stream's HTTP arm was a second refusal.** State A named only `link.rs`. The roster
  stream also refused `DaemonHttp` in `packages/tddy-discovery/src/roster/stream.rs`, before any
  client was asked, so that arm now joins the accepted transports. Its
  `TODO(session-agent-roster)` asked for a `StreamSessionAgents` client over HTTP, which
  `DaemonHttpRpcClient` now supplies, so the TODO was removed with it.
- **`DaemonHttpRpcClient` error mapping.** A non-200 body that is not a Connect error is `Unknown`,
  with the HTTP status and body. No code is guessed from the status. A stream that ends without its
  end-of-stream envelope is `Unavailable`. An envelope with an unrecognised flags byte is `Internal`.
- **The removed roster TODO is kept removed, pinned by a test.** `TODO(session-agent-roster)` in
  `packages/tddy-discovery/src/roster/stream.rs` is removed: the daemon-HTTP arm now carries the roster
  stream. `packages/tddy-daemon-rpc/tests/roster_stream_over_daemon_http_integration.rs` opens it over
  `DaemonHttp` against the daemon's real `/rpc` mount and asserts the roster the daemon holds is the one
  applied. The test fails with the old refusal restored (checked by a temporary mutation, reverted).
- **Remote placement is blocked: the developer's definition contradicts the code and the docs
  (2026-10-25).** The hand-off is [`2026-10-24-coagents-clone-placement.md`](./2026-10-24-coagents-clone-placement.md):
  its Decisions ("`open_owned` (peer-owned) and remote placement belong to n8 `remote-codebase`"), its
  Scope ("No remote placement — n8") and its TODO ("Carry to n8 `remote-codebase`: `open_owned`
  (peer-owned) and remote placement"). The definition for this node is: a subagent attached in
  remote-codebase mode runs on the codebase host's daemon, on a clone that daemon owns (`open_owned`),
  with results reported back over the hop. It does not reconcile with the branch or the docs:
  1. `open_owned` runs an agent this daemon owns **reading the clone it holds for another daemon's
     session** (`packages/tddy-session-agents/src/ports.rs:152-153`;
     `packages/tddy-session-agents/src/svc_start_hosted_agent_clone.rs:236-237`). A workspace session
     belongs to the codebase daemon, so that daemon holds no such clone. `open_owned` returns `None`
     (`packages/tddy-session-agents/src/service.rs:857`), and the codebase daemon's own agent takes the
     local branch (`service.rs:878`), which reads the authoritative worktree.
  2. An agent owned by the codebase host "is co-located and gets no clone"
     (`docs/ft/daemon/session-agent-roster.md:487-488`), and AC55 says it "succeeds with **no clone**:
     that agent reads the authoritative worktree directly" (`session-agent-roster.md:1105`).
  3. Workspace admission refuses any coding agent another daemon owns
     (`packages/tddy-session-agents/src/workspace_roster.rs:21-27`; this changeset's test 8), and the
     roster spec says "A coding agent owned by another daemon is not supported"
     (`session-agent-roster.md:218`). Remote placement of a coding agent has no workspace-session case
     to run in.
  4. The PRD says "this agent works on the authoritative worktree, not on a clone of someone else's"
     (`docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md:53-54`).

  The PRD's other use of "clone" is "its own clone with `clone` placement"
  (`docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-remote-codebase.md:44-45`). That is the
  per-conversation worktree of `AgentPlacement::Clone`, which the open path serves through `open_local`
  with the conversation id (`packages/tddy-session-agents/src/ports.rs:165-166`), not through
  `open_owned`. So "clone" and "(open_owned)" in the definition name different things. Not implemented;
  no failing tests written. **Developer decision needed:** (a) co-located only, which the code already
  does, with remote placement moved to a node that changes the admission rule; (b) "clone" means the
  per-conversation worktree, and the `(open_owned)` label becomes `open_local`; or (c) peer-owned coding
  agents on workspace sessions, which reverses `workspace_roster_admits` and the roster spec's "not
  supported" — a design change the developer must approve.

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

- [x] Record initial discovery (`2026-10-24-coagents-remote-codebase-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p tddy-tools -p tddy-session-tool-client -p tddy-session-agents -p tddy-daemon-rpc -p tddy-tools-cli`) — verify 100% pass
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
