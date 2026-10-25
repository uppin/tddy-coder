# Changeset: spawn_conversation starts any session through the universal start

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — reroutes a live spawn path (grill-me) through the start core, three or more crates (tddy-agent-launch, tddy-tools, tddy-host-service, tddy-tool-engine), and changes which sessions are offered the start tools
**PR**: [#758](https://github.com/uppin/tddy-coder/pull/758)
**Stack**: `#start-parity` 9/9 — branch `feature/start-parity/spawn-conversation`, base `n8` `disk-refresh` (`feature/start-parity/disk-refresh`)
**PRD**: [PRD-2026-10-25-start-parity-spawn-conversation.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-spawn-conversation.md)

## Initial Discovery

See [./2026-10-25-start-parity-spawn-conversation-initial-discovery.md](./2026-10-25-start-parity-spawn-conversation-initial-discovery.md).

## Responsibility

- Rerouting the grill-me spawn handler onto the start core.
- The tddy-tools args and subcommands.
- Offering the start tools to spawn-capable sessions.

## Boundaries

- Does not widen which recipes get the spawn capability.
- Does not change the start tools' schemas (those belong to `start-tool`, `async-start` and
  `start-options`).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` `start-tool` (`feature/start-parity/start-tool`) | `StartSessionArgs`, the `StartSession` tool, its schema, and the orchestrator fields 44/45 | builds on `StartSessionArgs` and the `StartSession` tool | change the args model, the schema generator, or the proto fields |
| `n4` `async-start` (`feature/start-parity/async-start`) | the start ledger, `StartSession` returning a handle, and `GetSessionStart` / `CancelSessionStart` | the spawn path returns the same handle and is polled and cancelled through the same ledger | add a second ledger or a status channel |

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `tddy-toolcall` `ConversationSpawnHandler` gains `start_conversation(args_json)`,
  `conversation_start_status(start_id)` and `cancel_conversation_start(start_id)`.
  - Each has a default body that fails with `TODO(spawn-conversation)`, so no implementor breaks.
  - The arguments and answers are JSON (`StartSessionArgs` in; `StartHandle` and
    `StartLedgerRecord` out), because `tddy-toolcall` does not depend on `tddy-tool-engine`.
  - The old `spawn_conversation(prompt, branch, base_ref)` stays until the relay and the MCP tool
    move over at green.
- `GrillMeConversationSpawnHandler.service: LaunchSessions`, the start path, filled at its three
  construction sites (two in production, one in the test harness). This mirrors
  `StackChildSpawnHandler.service`.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no — its tests exercise `n2` `start-tool` (`feature/start-parity/start-tool`)'s behaviour; its tests exercise `n4` `async-start` (`feature/start-parity/async-start`)'s behaviour, which must be green first
**Concurrent with:** `n8` `disk-refresh` (`feature/start-parity/disk-refresh`)
**Blocks:** nothing

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

— (top of the stack)

## Affected Packages

- [ ] `tddy-agent-launch`
- [ ] `tddy-tools`
- [ ] `tddy-toolcall` — the handler trait's universal-start methods
- [ ] `tddy-session-lifecycle` — handler tests
- [ ] `tddy-tool-engine`
- [ ] `tddy-daemon-rpc`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-spawn-conversation.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-spawn-conversation.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

`spawn_conversation` is a second, narrower start path: grill-me only, claude-cli only, local
only, with no progress and no cancel. This node puts it on the universal start. It takes
`StartSession`'s arguments, goes through `start_session_on_request`, and returns the async handle.
Sessions that can spawn conversations are also offered the start tools, limited to their own
project.

## Scope

- [ ] `spawn_conversation` can start any session type, on any eligible host, with every New-session option.
- [ ] It returns a handle, and the caller can poll and cancel the start.
- [ ] A session that can spawn conversations is offered the start tools, limited to its own project.

## Technical Changes

### State A

- The MCP tool `spawn_conversation` (`tddy-tools/src/server.rs:1001-1023`) and the CLI command
  (`cli.rs:353-385`) take {prompt, branch, base_ref} and send them to the toolcall socket.
- From there: `GrillMeConversationSpawnHandler` (`tddy-agent-launch/src/conversation_spawn_handler.rs:13-97`),
  enabled only for recipe `grill-me` (`conversation_spawn.rs:15`).
- The handler calls `spawn_claude_cli_session_inner` directly, with a discarding sink. It returns
  the child's id.

### State B

- `spawn_conversation` (MCP and CLI) takes `StartSessionArgs` (from `start-tool`), with
  `stack_parent` defaulting to the caller.
- The handler builds a `StartSessionRequest` and runs it through the async start (`async-start`),
  so it can start any session type on any eligible host, with progress and cancel. It returns
  `{start_id, task_id, daemon_instance_id}`.
- New tddy-tools subcommands `spawn-conversation-status` and `spawn-conversation-cancel`, and
  their MCP counterparts.
- What is offered: a session with the conversation-spawn capability is offered `StartSession`,
  `GetSessionStart`, `CancelSessionStart` and `DescribeSessionStartOptions`, with `project_id` fixed
  to its own project. The grill-me binding is kept as the capability's *source*, not as a limit on
  what can be started.

## Testing Plan

### Acceptance Tests

- [ ] **Unit** `packages/tddy-tools/tests/spawn_conversation_universal.rs`:
  - `spawn_conversation_accepts_every_start_session_argument` — the MCP schema ⊇ `StartSession`'s
    offered fields
  - `a_spawned_conversations_start_can_be_followed_and_cancelled` — `spawn_conversation_status`
    and `spawn_conversation_cancel` are advertised and take `start_id`
- [ ] **Integration** `tddy-session-lifecycle` `stack_child_spawn_tests.rs` (the existing grill-me
  handler harness):
  - `a_conversation_of_any_session_type_is_started_through_the_universal_start` — a `workspace`,
    on the named branch, linked to the orchestrator
  - `a_conversation_start_the_start_path_refuses_is_the_agents_error`
  - `an_unknown_conversation_start_is_refused_naming_it`
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/spawn_capable_session_tools_acceptance.rs`:
  - `a_session_that_spawns_conversations_is_offered_the_start_tools`
  - `an_ordinary_session_is_not_offered_them`
  - `a_spawning_session_starts_in_its_own_project_linked_to_itself`

**Green notes:**
- **Authentication.** `start_session_on_request` authenticates a session token, but the grill-me
  handler holds an `os_user`, not a token. Green either gives the handler the orchestrator's start
  token (it already has the owner's `account_access`) or reaches the start core below the token
  check, as the handler reaches `spawn_claude_cli_session_inner` today.
- **Cross-host spawn.** It is not tested here beyond what `StartSession` already proves. The handler
  runs the same start, so a `daemon_instance_id` in the arguments takes the same forward.

Tests are written in the `fluent-tests` style (Given / When / Then). 

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **One universal start** (developer, Q4): nothing about what can be started is limited — no
  grill-me-only, claude-cli-only or local-only rule. Who may start stays: project-home and
  spawn-capable sessions, each limited to its own project (Q2).

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-spawn-conversation-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer: "finish the remaining nodes without stopping", 2026-10-25)
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
