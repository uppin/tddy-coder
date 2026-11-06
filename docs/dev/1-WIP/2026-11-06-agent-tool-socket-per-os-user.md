# Changeset: Agent-tool socket per OS user

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — supervisor protocol change (FD hand-over contract) + several crates + socket ownership across OS users

## Initial Discovery

[2026-11-06-agent-tool-socket-per-os-user-initial-discovery.md](./2026-11-06-agent-tool-socket-per-os-user-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-03-server-daemon-agent-tool-socket-per-os-user.md](../todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md) | ✅ RESOLVED HERE — design A chosen by the developer (2026-11-06) |

Starting point: the local ref `wip/agent-tools-cursor-cli-server-socket` (`07128a63e`) — server
binding in `build_context.rs`, `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs`
(3 tests). It was cut on #796; it is re-applied onto this branch, not merged.

## Affected Packages

- **tddy-supervisor** — `agent_tool_sockets` config, validation, hand-over as `agent-tools.<user>`, FD-name prefix pinned.
- **tddy-daemon** — `AgentToolSockets` registry (bind owner-only / adopt inherited); server binding.
- **tddy-agent-launch**, **tddy-session-split**, **tddy-session-lifecycle**, **tddy-session-tool-client**, **tddy-tools** — consumers take the per-user socket; refuse unlisted users.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-agent-tool-socket-per-os-user.md](../../ft/daemon/1-WIP/PRD-2026-11-06-agent-tool-socket-per-os-user.md)

## Summary

Server daemons serve the agent-tool socket per OS user (design A), handed over by the supervisor
like host-session sockets; consumers use the session user's socket and refuse a user without one.

## Scope

- [ ] Re-apply the held server binding and its 3 tests
- [ ] Supervisor: `agent_tool_sockets: [{user, path}]`, validation (allowed user, distinct dir, `MAX_HANDED_OVER_LISTENERS`), `agent-tools.<user>` FD names, prefix pinned in both crates
- [ ] Daemon: `AgentToolSockets` (twin of `HostSessionSockets`), `<data dir>/run-agent-tools/<user>/agent.sock`, `bind_owner_only` shared at `pub(crate)`, inherited-listener adoption
- [ ] Consumers: `embedded_host_agent_tool_socket` replaced by `AgentToolSockets::ensure_bound(user, data_dir)`; unlisted user under the supervisor refused at start, naming the setting
- [ ] Claude's and Cursor's host suites on a server daemon

## Technical Changes

### State A

See the record (design A, points 1–4) and `tddy-daemon/src/runtime/build_context.rs:148-153` (binds only for `RuntimeHost::Embedded`). Host-session analogue: `tddy-agent-launch/src/host_session_socket.rs`, supervisor `socket.rs`, `host_socket_config.rs`.

### State B

As the record's design A. `AgentToolSockets` is threaded through `LaunchSessions` /
`DaemonSessionHost` beside the non-optional `coding_agents` (`feature/agent-tools/host-registry`).

## Testing Plan

Supervisor unit tests for config validation and FD naming; daemon integration tests with two OS
users simulated by directories and modes (real `chown` only where the test runs privileged, else
skipped with a named reason, as the host-session tests do); consumer tests assert the socket chosen.

## Acceptance Tests

| Test | File |
|---|---|
| held 3 tests (server binds and serves) | `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` |
| `a_session_running_as_x_is_served_on_xs_agent_tool_socket` | `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` |
| `an_unlisted_user_under_the_supervisor_is_refused_at_start_naming_the_setting` | `packages/tddy-agent-launch/tests/agent_tool_socket_per_user.rs` |
| `supervisor_config_rejects_a_shared_dir_and_too_many_listeners` | `packages/tddy-supervisor/src/host_socket_config.rs` (`#[cfg(test)]`) |
| `agent_tool_listeners_are_announced_as_agent_tools_user` | `packages/tddy-supervisor/src/socket.rs` (`#[cfg(test)]`) |

## Responsibility

The per-OS-user agent-tool socket: supervisor hand-over, daemon binding, consumer selection.

## Boundaries

- Does **not** change the host-session socket or its two RPCs.
- Does **not** change the embedded (desktop) socket.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/host-registry` | `DaemonSessionHost` with a non-optional `coding_agents` and a `new` that takes it | adds `AgentToolSockets` beside it | change registry loading |

## Draft PR contract

The first push after this plan: the held binding re-applied, `AgentToolSockets` and the supervisor
config types as compiling surface, plus the acceptance tests above failing.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — threads beside `feature/agent-tools/host-registry`'s host constructor
**Concurrent with:** `feature/agent-tools/split-remote-env`, `feature/agent-tools/acp-subagent-leftovers`
**Blocks:** nothing

## Decisions & Trade-offs

- Design A (developer, 2026-11-06) over B: keeps `host.sock` at its two RPCs; costs a second handed-over listener per user, halving the per-host user capacity.
- Complexity 5: the supervisor FD contract is the boundary most likely to be wrong; if `/green` finds it needs its own PR, split the supervisor half below this node.

## TODO

- [x] Record initial discovery
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

## Technical Debt & Production Readiness

_(populated during development)_

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
