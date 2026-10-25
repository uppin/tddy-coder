# Changeset: spawn_conversation starts any session through the universal start

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — reroutes a live spawn path (grill-me) through the start core, three or more crates (tddy-agent-launch, tddy-tools, tddy-host-service, tddy-tool-engine), and changes which sessions are offered the start tools
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

- `SpawnConversationInput` becomes the type that holds `StartSessionArgs`.
- `ConversationSpawnHandler::spawn_conversation` returns `StartHandle`.
- New CLI args. Failing tests.

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
- [ ] `tddy-host-service`
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

- [ ] **Integration** `packages/tddy-agent-launch/tests/spawn_conversation_universal_acceptance.rs` — `a_grill_me_session_spawns_a_cursor_cli_conversation`: A session of type cursor-cli is created with the caller as `stack_parent`.
- [ ] **Integration** `same file` — `spawn_conversation_returns_a_handle_and_status_reaches_succeeded`: The status command polls to `succeeded` with session_id and worktree.
- [ ] **Integration** `same file` — `spawn_conversation_cancel_stops_the_start`: The status reads `cancelled`; no session dir remains.
- [ ] **Integration** `packages/tddy-daemon/tests/project_home_start_remote_acceptance.rs` — `a_conversation_is_spawned_on_another_host`: The peer holds the child; the orchestrator tag is on the peer.
- [ ] **Integration** `packages/tddy-daemon-rpc/tests/start_session_tool_mapping.rs` — `a_spawn_capable_session_is_offered_the_start_tools_for_its_own_project_only`: `ListExecTools` includes the four tools. A start naming another `project_id` is refused.
- [ ] **Unit** `packages/tddy-tools/tests/spawn_conversation_args.rs` — `spawn_conversation_accepts_every_start_session_argument`: The CLI and MCP schema properties ⊇ the `StartSession` schema properties.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
