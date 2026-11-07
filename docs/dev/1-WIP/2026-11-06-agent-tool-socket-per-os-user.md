# Changeset: Agent-tool socket per OS user

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — supervisor protocol change (FD hand-over contract) + several crates + socket ownership across OS users. The pass-2 contract assessed it at 6 (a cross-crate placement change, 7 consumer sites going from sync probes to an async-resolved path, golden churn in the host suites); kept at 5 by the user (2026-11-07), with the split contingency below.
**PR**: [#834](https://github.com/uppin/tddy-coder/pull/834)

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
- **tddy-daemon-kernel** — `AgentToolSockets` registry (bind owner-only / adopt inherited / serve the roster), `agent_tool_fds`, the per-user path. Moved here from the plan's `tddy-daemon` (see Decisions).
- **tddy-daemon** — runtime wiring: install the roster, pick the placement, adopt inherited `agent-tools.*` descriptors.
- **tddy-agent-launch**, **tddy-session-split**, **tddy-session-lifecycle**, **tddy-session-tool-client**, **tddy-tools** — consumers take the per-user socket; refuse unlisted users.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-agent-tool-socket-per-os-user.md](../../ft/daemon/1-WIP/PRD-2026-11-06-agent-tool-socket-per-os-user.md)

## Summary

Server daemons serve the agent-tool socket per OS user (design A), handed over by the supervisor
like host-session sockets; consumers use the session user's socket and refuse a user without one.

## Scope

- [x] The held server binding's 3 tests, per user (the held binding itself is NOT re-applied: design A replaces it)
- [x] Supervisor: `agent_tool_sockets: [{user, path}]`, validation (allowed user, distinct dir, `MAX_HANDED_OVER_LISTENERS`), `agent-tools.<user>` FD names, prefix pinned in both crates
- [x] Daemon: `AgentToolSockets` (twin of `HostSessionSockets`), `<data dir>/run-agent-tools/<user>/agent.sock`, `bind_owner_only` shared at `pub(crate)`, inherited-listener adoption
- [x] Consumers: `embedded_host_agent_tool_socket` replaced by `AgentToolSockets::ensure_bound(user, data_dir)`; unlisted user under the supervisor refused at start, naming the setting
- [x] Claude's and Cursor's host suites on a server daemon
- [x] Move `bind_owner_only`, its helpers (`ensure_dir`, `clear_stale_socket`, `chown`, `undo`) and the inherited-descriptor vetting (`adopt_descriptor`, `vet`, `require_listening`) from `tddy-agent-launch` to `tddy-daemon-kernel` as `pub`, with the refusal hint parameterised (`host_sockets:` / `agent_tool_sockets:`); `HostSessionSockets` calls the shared code
- [x] Remove `tddy_daemon_kernel::embedded_host_agent_tool_socket` and its 3 kernel unit tests once every consumer uses `ensure_bound`
- [x] Add `agent_tool_sockets:` entries to the install/e2e supervisor templates (`tddy-e2e/tests/install_supervisor.rs`, `./install`)
- [x] `tddy-stdio` from `[dev-dependencies]` to `[dependencies]` of `tddy-daemon-kernel` (serving the roster)

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
| `a_session_running_as_x_is_served_on_xs_agent_tool_socket_and_nobody_else_can_reach_it` | `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` |
| `a_server_daemon_tells_a_session_its_users_own_agent_tool_socket` | `packages/tddy-daemon/tests/server_agent_tool_socket_acceptance.rs` |
| `a_host_cursor_sessions_hooks_report_over_its_users_served_agent_tool_socket`, `a_host_claude_sessions_hooks_report_over_its_users_served_agent_tool_socket` | `packages/tddy-session-lifecycle/tests/agent_tool_socket_per_user_acceptance.rs` |
| `an_unlisted_user_under_the_supervisor_is_refused_naming_the_setting` | `packages/tddy-daemon-kernel/src/agent_tool_sockets_tests.rs` (moved from agent-launch: the registry lives in the kernel) |
| `supervisor_config_rejects_a_shared_dir_and_too_many_listeners` | `packages/tddy-supervisor/src/host_socket_config.rs` (`#[cfg(test)]`) |
| `agent_tool_listeners_are_announced_as_agent_tools_user` | `packages/tddy-supervisor/src/supervisor.rs` (`#[cfg(test)]`; moved from `socket.rs`: the handover is built from a `Supervisor`'s slot) |
| `reads_the_same_agent_tool_descriptor_names_the_supervisor_writes` | `packages/tddy-session-lifecycle/src/connection_service/inherited_host_sockets_tests.rs` (FD-name prefix pinned across both crates) |

The held 3 tests are re-applied in `server_agent_tool_socket_acceptance.rs`, moved from the one digest-named socket to the session user's. The held **binding** (`07128a63e`, a server binding the daemon user's digest socket) is **not** re-applied: under design A it is exactly the EACCES defect the record describes.

Unit tests (red): `tddy-daemon-kernel/src/agent_tool_sockets_tests.rs` (path, modes, roster, sharing, inherited, FD parsing, path refusals); `tddy-supervisor/src/host_socket_config.rs` (validation parity), `supervisor.rs` (`creates_every_declared_agent_tool_socket_when_it_is_built`). Guards that pass now: `finds_no_agent_tool_descriptors_when_the_variables_describe_another_process`, `an_embedded_host_tells_every_session_the_runtimes_one_socket`, `accepts_a_host_and_an_agent_tool_socket_for_one_user_in_directories_of_their_own`, `declares_no_agent_tool_sockets_by_default`, `the_agent_tool_fd_name_prefix_is_not_the_host_session_one`.

**Linux/CI only (not testable unprivileged on macOS):** a second real OS user being refused a `connect` to another user's socket; a root supervisor handing a socket to another account; an unlisted user refused under a real supervisor. These belong to the Linux fork-based or VM suites; on macOS they are pinned by mode/owner checks and by an unprivileged bind for `root`.

## Responsibility

The per-OS-user agent-tool socket: supervisor hand-over, daemon binding, consumer selection.

## Boundaries

- Does **not** change the host-session socket or its two RPCs.
- Does **not** change the embedded (desktop) socket.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/host-registry` | `DaemonSessionHost` with a non-optional `coding_agents` and a `new` that takes it | adds `AgentToolSockets` beside it, as a field defaulted in `new` plus `with_agent_tool_sockets` / `agent_tool_sockets()` — `new`'s signature is unchanged | change registry loading |
| `feature/agent-tools/split-remote-env` | `SplitSessions.session_pointers`, the split agent's remote via its tool server | adds `SplitSessions.agent_tool_sockets` beside it | touch the pointer or the remote env |

No test of this node reaches a parent `TODO(`: every red failure is this node's own `TODO(agent-tool-socket-per-user)`.

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
- **Shape** (user, 2026-11-07, via orchestrator AskUserQuestion): "Approve, split at green" — one node; if `/green` runs long, the supervisor half (config, handover, bind) becomes its own node below this one. Scored 5; pass 2 assessed 6.
- **Placement** (user, 2026-11-07, via orchestrator AskUserQuestion): `AgentToolSockets` lives in `tddy-daemon-kernel`, and the internal `tddy-stdio` dependency is approved. The plan's `tddy-daemon` was impossible: the consumers (`tddy-agent-launch`, `tddy-session-split`) are below it, and `bind_owner_only` at `pub(crate)` in `tddy-agent-launch` cannot reach `tddy-session-split`, a dependency of that crate.
- **Moves** (user, 2026-11-07, via orchestrator AskUserQuestion): all approved — `bind_owner_only`, its helpers and the inherited-descriptor vetting move to the kernel as `pub` with a parameterised refusal hint; `embedded_host_agent_tool_socket` and its 3 kernel unit tests are removed once every consumer uses `ensure_bound`; `agent_tool_sockets:` entries are added to the install/e2e supervisor templates.
- **Behaviour** (user, 2026-11-07, via orchestrator AskUserQuestion): "Refuse; empty roster OK" — (a) under the supervisor an unlisted user's start is **refused**, naming `agent_tool_sockets` in `supervisor.yaml` (unlike host-session sockets, which log and start without one); (b) a `DaemonSessionHost` without a runtime roster binds and accepts with an empty roster.

### Pass-2 premise check (against #830's landed code, 2026-11-07)

- Held: `DaemonSessionHost::new` takes a non-optional `coding_agents`; the registry loads once in `runtime::build`; `svc_host_builders` / `svc_room_builders` split (the builders went into `svc_host_builders.rs`).
- Stale: State A's line numbers — the Embedded gate is `build_context.rs` in `serve_common_room` (~155-165).
- Wrong: `AgentToolSockets` in `tddy-daemon` with `bind_owner_only` at `pub(crate)` (see Placement).
- Stale: the consumer list — `chat.rs` `cursor_notify_env` was deleted by node 24 (#agent-tools 24/28), and there is no `spawn_cursor_from_spec`. The **7 live consumer sites**: `tddy-agent-launch` `claude_cli_spawn_steps.rs` (`claude_cli_hooks`), `notify_env.rs` (`agent_notify_launch`), `project_home_launch.rs`, `cli_agent_steps.rs` (`socket_for_hooks`, Cursor, names the digest path unprobed); `tddy-session-split` `coding_agent_wiring.rs`, `svc_spawn_split_agent.rs` (2 calls of `agent_tool_socket_for_embedded_host`), `svc_start_sandboxed_codebase_session.rs`. All are sync and build from `tddy_data_dir`; `wire_session_agent` already carries `os_user`, so `/green` resolves the async `ensure_bound` upstream and passes the path down.
- Wrong: "re-apply the held server binding" (see Acceptance Tests).

### Expected golden churn

On a server daemon the hooks switch from HTTP to the user's socket, so `/green` updates the host Claude goldens (e.g. `claude_cli_from_spec_acceptance`'s `host_start_settings_local.json`) and the `on_a_regular_host_*` unit tests in `claude_cli_spawn_steps_tests.rs` / `notify_env_unit_tests.rs`.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
