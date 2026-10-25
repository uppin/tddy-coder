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
- Remote placement (developer decision, 2026-10-25, recorded under Decisions): a subagent attached in remote-codebase mode runs on the codebase host's daemon, on the per-conversation worktree that daemon serves for `clone` placement through `open_local`, with results reported back over the hop. There is no `open_owned` path; the workspace admission rule is unchanged.

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

**Remote placement — tests 10–12** in `packages/tddy-daemon-rpc/tests/remote_codebase_clone_placement_acceptance.rs`:
a clone-placed coding agent of a workspace session opens on its conversation worktree (10); a diff asked through
the relay's HTTP endpoint is answered by the codebase host it forwards to (11); a pull asked through the relay applies
the conversation's change to the codebase host's session worktree (12). All three pass on arrival: the premise already
holds on this branch, so no production surface changed for them.

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

- ~~**`remote` requests are JSON, which the relay cannot decode (blocks `--agent` end to end).**~~
  **Resolved in `c29cfb34a`:** `connect_post` sends `application/proto` with the protobuf body
  (`packages/tddy-tools/src/remote_cli.rs:125-143`), and the HTTP arms of `ExecuteTool`, `ListExecTools`
  and `ConversationWorktree` send protobuf too (Decisions). Not checked against a live daemon.
- **The `remote` output is not pinned by any test.** The printed JSON comes from the `ConnectJson`
  builders in `packages/tddy-tools/src/remote_cli.rs` (the section "Connect JSON of a decoded response"),
  and no test compares it with the output the CLI printed before its wire was protobuf.
- **`ConversationWorktree` over HTTP: covered in-process; the LiveKit hop is NOT RUN.**
  Tests 11 and 12 (`remote_codebase_clone_placement_acceptance.rs`) send it through the relay's HTTP
  endpoint, in-process, and pass. The entry in `docs/dev/todo` is not edited here. Prerequisite text
  "Test 6 exercises it" is stale: test 6 is roster admission. The two-daemon suite below adds the LiveKit hop, which is NOT RUN.
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
  seeding gap above. **Not run (2026-10-25):** `LIVEKIT_TESTKIT_WS_URL` is unset in this environment, so
  no LiveKit testkit was started and nothing was faked. The two-daemon suite is recorded as NOT RUN.
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
- **Remote placement: resolved by the developer (2026-10-25).** The earlier block came from a hand-off
  that said remote placement runs on `open_owned`, a clone this daemon holds for a peer's session. A
  workspace session belongs to the codebase daemon, which holds no such clone, and the roster spec and
  the workspace admission rule refuse peer-owned coding agents. The developer's reading: `clone` on a
  remote-codebase session is the per-conversation worktree the codebase daemon serves through
  `open_local`, with the conversation id (`packages/tddy-session-agents/src/ports.rs:165-166`;
  `open_coding_agent_session` in `packages/tddy-session-agents/src/coding_agent.rs`). The `open_owned`
  label is dropped from this changeset, and the hand-off's `open_owned`
  ([`2026-10-24-coagents-clone-placement.md`](./2026-10-24-coagents-clone-placement.md)) resolves to this
  reading. The admission rule is unchanged: a workspace session's roster holds only the coding agents of
  the daemon serving it. Pinned by `packages/tddy-daemon-rpc/tests/remote_codebase_clone_placement_acceptance.rs`
  (tests 10–12), which pass on arrival.
- **Accepted: axum 0.8 dev-dependency (developer-approved).** `axum = "0.8"` is a dev-dependency of
  `tddy-session-tool-client` and `tddy-daemon-rpc`. It serves the daemon's `/rpc` mount on a loopback port
  in the roster-stream and remote-placement tests. Nothing else enters the tree.
- **Accepted behaviour changes.** The `remote` output is canonical proto3 JSON with proto3 defaults omitted.
  Its keys are camelCase before and after: the pre-change CLI printed the daemon's JSON body, and its
  fixtures in `remote_cli_subcommand_acceptance.rs` (at `95bce3dfd`) read `sessionId`, `livekitRoom`,
  `livekitUrl` and `livekitServerIdentity`, which the `ConnectJson` builders still write. So no key style
  changed, and no format pin was needed. Whether the default omission matches the pre-change bodies byte
  for byte is not compared. A missing `result_json` is an error on every conversation transport (see
  above).
- **Two-daemon suite: NOT RUN (2026-10-25).** The LiveKit two-daemon suite is not written and was not run.
  `LIVEKIT_TESTKIT_WS_URL` is unset here, so no LiveKit testkit was started, and nothing is faked. The
  remote-placement behaviour is covered in-process instead: `remote_codebase_clone_placement_acceptance.rs`
  puts the relay and the codebase host in one process, with an in-process forwarder standing in for the
  common room's LiveKit transport. NOT RUN: the LiveKit hop and the real two-daemon handshake.

- **`remote` output is canonical proto3 JSON, defaults omitted (behaviour change).** `tddy-tools remote`
  decodes each answer from protobuf and prints it through the `ConnectJson` builders in
  `packages/tddy-tools/src/remote_cli.rs`, which write camelCase keys and leave proto3 defaults out. The
  example `{"livekitRoom":"room-x","sessionId":"sess-full"}` is the start-session builder's output for those
  two fields; no test pins that string. Before, `connect_post` printed the daemon's JSON answer as it
  arrived (`git show master:packages/tddy-tools/src/remote_cli.rs`, `println!("{}", resp)` at lines 228,
  258 and 287). Whether the printed keys or defaults differ from that output has not been compared byte for
  byte.
- **Protobuf on the `remote`, `ExecuteTool`, `ListExecTools` and conversation HTTP wires (behaviour
  change).** `remote` requests go as `application/proto` (`remote_cli.rs:142`). `ExecuteTool` and
  `ListExecTools` over the daemon's HTTP endpoint (`DaemonHttpRpcClient`,
  `packages/tddy-session-tool-client/src/daemon_http_rpc.rs`) and the conversation HTTP arm
  (`packages/tddy-session-tool-client/src/conversation.rs`, `ask_daemon_over_http`) send protobuf bodies and
  decode protobuf answers. The error wording on those paths:
  - `ListExecTools` now reports `tool rpc call: …` and `decode ListExecToolsResponse: …`
    (`packages/tddy-session-tool-client/src/exec_catalog.rs:147,149`). Master reported `relay connection
    error`, `relay answer is not JSON` and `the daemon answered {status}`.
  - The conversation HTTP arm now reports `conversation worktree rpc call: …` and `conversation worktree rpc
    decode response: …` (`conversation.rs:237,247`), the wording its RPC arm already used. Master reported
    `relay answered without a result`.
  - `ExecuteTool` over HTTP changed its body to protobuf and kept its `relay connection error: …` and
    `relay parse error: …` wording (`packages/tddy-session-tool-client/src/rpc_transports.rs:147,157`). It
    does not use the shared `tool rpc call` wording.
- **An empty `result_json` is an error on all four conversation transports (behaviour change).**
  `ask_daemon` (`conversation.rs:243-244`) answers `{"error": "the reply carries no result_json",
  "is_error": true}` when a reply's `result_json` is empty. Every conversation transport goes through it:
  sandbox IPC, daemon UDS, LiveKit (`ask_daemon_over_livekit`, `conversation.rs:209`) and HTTP
  (`ask_daemon_over_http`). Before, the RPC arms returned the empty string. The alternative not taken was
  `optional string result_json` in the proto, a schema change that needs the developer's approval.
- **Open question, no decision: admission of seeded agents on workspace sessions.** `workspace_roster_admits`
  has no production caller, so a workspace start's seeded agents are not checked against it (see Technical
  Debt). Whether seeded agents on a workspace session go through admission, and how a split session is
  treated, is open. The split question is defined in the orchestrator's brief and is not decided here.

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
