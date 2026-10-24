# Changeset: Coding subagents on the codebase host, in remote-codebase mode

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — four crates, relay forwarding (a cross-daemon RPC seam), a roster on a new session type
**Branch**: `feature/coagents/remote-codebase`, base `feature/coagents/clone-placement` (#724) — `#coagents` 6/8
**PR**: _assigned at wave 1_

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

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — needs n2's coding-agent conversation behaviour
**Concurrent with:** n3, n5, n7
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

(n8 also reads n7's `placement` field — published surface, not behaviour; its clone test 6 needs n7 green.)

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

`packages/tddy-daemon-rpc/tests/remote_codebase_subagent_acceptance.rs` (new):

1. `a_workspace_session_started_with_a_coding_agent_has_it_on_its_roster`
2. `a_workspace_session_refuses_a_model_agent_or_a_foreign_owner`
3. `a_conversation_through_the_relay_runs_on_the_workspace_worktree`
4. `prompt_await_permission_and_cancel_cross_the_relay`
5. `a_workspace_session_without_agents_has_no_roster`
6. `a_clone_placed_agent_through_the_relay_pulls_into_the_workspace_worktree`

`packages/tddy-tools-cli/tests/remote_cli_subcommand_acceptance.rs` (extend):

7. `remote_start_session_passes_each_agent`
8. `subagent_tools_are_advertised_in_remote_mode_with_a_roster`

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Remote-codebase mode as built, not as specified.** Criteria 23–28 (`tddy-coder --remote`) are
  unimplemented; the user-facing opt-in is therefore `tddy-tools remote start-session --agent`.
  Wiring `tddy-coder --remote` remains its existing TODO.
- **Coding agents only, local owner only.** A `model` agent gains nothing from the codebase host, and
  a third daemon would be multi-hop (roster non-goal).

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
