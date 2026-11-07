# Changeset: Split sessions hand their remote to the tool server and the session pointer, and render the host's agent spec

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**PR**: [#833](https://github.com/uppin/tddy-coder/pull/833) (`#agent-tools 25/28`)
**Complexity**: 3 — the production change is concentrated in tddy-session-split (3 env TODO sites, one registry lookup, the pointer); its size is the test migration (6 tests in 2 suites) and the spec plumbing, both already in the contract

## Initial Discovery

[2026-11-06-split-remote-env-via-pointer-initial-discovery.md](./2026-11-06-split-remote-env-via-pointer-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-03-split-sessions-render-the-embedded-agent-spec.md](../todo/2026-11-03-split-sessions-render-the-embedded-agent-spec.md) | ✅ RESOLVED HERE (both halves: host spec, `TDDY_*` off the process) |

No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-session-split** — `agent_argv.rs` (`split_agent_spec(registry)`, `split_claude_extra_args(spec, …, tool_server_env, …)`); `split_session.rs` (`prepare_split_agent_wiring`, `build_split_context_dir`, `split_session_pointer`, `SplitAgentWiring.session_pointer`); `split_ports.rs` (`SplitSessions.coding_agents`, `SplitSessions.session_pointers`, `SplitSessionPointers` port); `svc_spawn_split_agent.rs`, `svc_resume_split_wiring.rs`, `svc_start_sandboxed_codebase_session.rs`.
- **tddy-agent-launch** — `split_session_pointers.rs` (`HostSplitSessionPointers`: #829's `place_session_pointer` / `removing_session_pointer_on_exit`, local host only); `opencode_cli_spawn/resume.rs` places `split.session_pointer`; `project_home_launch_tests.rs` moves off `tddy_tools_only_mcp_args`.
- **tddy-session-lifecycle** — `handler_state.rs` gives `SplitSessions` the host registry and the pointer port; `test_util::coding_agents_with_binaries_over`.
- **tddy-coding-agent-specs** — `embedded_spec` loses every production caller (`TODO(jail-agnostic)` resolved).
- **tddy-daemon** (tests) — `split_session_resume_acceptance.rs`.
- **tddy-daemon-rpc** (test) — `session_agent_replacement_acceptance.rs` call-site update.

## Related Feature Documentation

None — the agent sees the same remote; only the channel changes. No PRD.

## Summary

A split session's agent process gets no `TDDY_*` variable — neither `TDDY_REMOTE_*` nor the roster
seed. Its `tddy-tools --mcp` server is registered with them, and its context dir holds a
`.tddy/session.json` pointer naming the same transport, owned by this daemon, removed when the agent
exits and when the session is deleted. The split agent's argv, hooks, context globs and guidance
files are rendered from the host's registry (operator overrides included), not the embedded spec.

## Scope

- [ ] No `TDDY_*` on the split agent's process: the transport and the roster seed go to the `tddy-tools` MCP registration env (start, resume, sandboxed-codebase resume)
- [ ] A context-dir pointer naming the transport (owner {pid, host}, remote map), 0600, git-excluded where a repo exists, removed on exit and on delete; rewritten on resume
- [ ] `split_agent_spec` reads the host registry; every split caller passes the resolved spec
- [ ] `embedded_spec` has no production caller
- [ ] `tddy_tools_only_mcp_args` deleted; `project_home_launch_tests.rs:174` moved off it

## Technical Changes

### State A

- The split agent's process env carries `TDDY_REMOTE_*` and `TDDY_SESSION_AGENT_ROSTER_SEED`:
  `svc_spawn_split_agent.rs` (`start_with_options(… remote.env_pairs().chain(roster_seed) …)`),
  `opencode_cli_spawn/resume.rs` (`env.extend(split_env)`), `svc_start_sandboxed_codebase_session.rs`.
  Its `tddy-tools --mcp` registration (`agent_argv::tddy_tools_server`) carried only the log file and
  `RUST_LOG`, and inherited the transport from the process.
- `jail_launch.rs` already routes `TDDY_*` to the MCP env only (premise of the plan was wrong).
- No pointer in the split context dir: the split start bypasses `cli_agent_steps::session_pointer_of`.
- `split_agent_spec()` → `embedded_spec("claude")`, 7 production call sites (argv, hooks, context
  globs ×3, `build_split_context_dir` guidance with an `unwrap_or_default` fallback,
  `tddy_tools_only_mcp_args`).

### State B

- `split_claude_extra_args(spec, session_dir, tddy_tools_path, tool_server_env, withdrawals)`
  registers `tool_server_env` (transport + roster seed) on `tddy-tools`; `SplitAgentWiring.env` and
  the start's process env hold no `TDDY_*`.
- `split_session_pointer(remote, context_dir, session_dir)` builds the pointer;
  `SplitAgentWiring.session_pointer` carries it; the start places it through
  `SplitSessions.session_pointers` and spawns with the exit hook that removes it
  (`CliSessionManager::spawn_rendered_main` takes `on_exit`); the resume places
  `split.session_pointer` in the context dir with #829's helpers. The context dir is inside the
  session dir, so a delete removes it with the session.
- `split_agent_spec(&CodingAgentRegistry)` reads `SplitSessions.coding_agents`.

## Testing Plan

In-memory wiring and port tests in tddy-session-split and tddy-agent-launch; the real two-daemon
resume suite (LiveKit testkit) for the relaunched process. Six existing tests — nine assertion
sites — pinned the old delivery and now read the MCP registration instead (red until green):

- `tddy-daemon/tests/split_session_resume_acceptance.rs`: `a_resumed_split_agent_is_pointed_at_the_codebase_hosts_session_not_its_own`, `a_resumed_split_agents_tool_transport_is_rebuilt_from_the_persisted_pairing`, `a_resumed_split_agent_receives_a_join_token_minted_at_the_resume`, `a_split_sessions_agent_half_resumes_through_the_split_path_not_the_sandboxed_runner_path` (and its fixture's `agent_env_var` helper is deleted)
- `tddy-session-lifecycle/tests/session_room_acceptance.rs`: `a_split_agent_is_wired_to_the_session_room_rather_than_the_lobby`, `a_split_agent_addresses_the_daemon_that_hosts_its_room`

(The resume suite's four tests hold seven of the migrated reads, `transport_is_rebuilt…` four of
them; earlier reports counted these as seven tests.)

## Acceptance Tests

| Test | File |
|---|---|
| `a_split_agent_process_carries_no_tddy_env` | `packages/tddy-session-split/tests/split_agent_wiring_acceptance.rs` |
| `a_split_agents_tools_reach_the_remote_through_their_tool_server` | `packages/tddy-session-split/tests/split_agent_wiring_acceptance.rs` |
| `a_split_agents_context_dir_pointer_names_its_transport` | `packages/tddy-session-split/tests/split_agent_wiring_acceptance.rs` |
| `a_split_agents_flags_still_withdraw_what_its_roster_took_over` (guard) | `packages/tddy-session-split/tests/split_agent_wiring_acceptance.rs` |
| `a_split_session_renders_the_hosts_overridden_agent_spec` | `packages/tddy-session-split/src/agent_argv.rs` (`host_spec_tests`) |
| `a_resumed_split_agent_process_carries_none_of_its_tool_transport` | `packages/tddy-daemon/tests/split_session_resume_acceptance.rs` |
| `a_resumed_split_session_renders_the_hosts_agent_spec` | `packages/tddy-daemon/tests/split_session_resume_acceptance.rs` |
| `a_resumed_split_agents_context_dir_holds_a_pointer_rewritten_for_it` | `packages/tddy-daemon/tests/split_session_resume_acceptance.rs` |
| `a_deleted_split_session_leaves_no_pointer_behind` | `packages/tddy-daemon/tests/split_session_resume_acceptance.rs` |

### Unit tests

| Test | File |
|---|---|
| `a_host_without_the_split_agent_refuses_a_split_session` | `tddy-session-split/src/agent_argv.rs` |
| `a_split_agents_tool_server_is_registered_with_the_sessions_transport` | `tddy-session-split/src/agent_argv.rs` |
| `a_split_agents_tool_server_keeps_its_log_settings_beside_the_transport` (guard) | `tddy-session-split/src/agent_argv.rs` |
| `a_host_with_no_overrides_renders_the_builtin_host_variant` (guard) | `tddy-session-split/src/agent_argv.rs` |
| `a_split_session_pointer_carries_its_transport_by_variable_name` | `tddy-session-split/src/split_session.rs` (`split_session_pointer_tests`) |
| `a_split_session_pointer_is_owned_by_this_daemon_on_this_host` | `tddy-session-split/src/split_session.rs` |
| `a_split_session_pointer_names_its_context_dir_and_no_socket` (guard) | `tddy-session-split/src/split_session.rs` |
| `a_split_agents_context_dir_holds_its_pointer_owner_only` | `tddy-agent-launch/src/split_session_pointers/split_session_pointers_tests.rs` |
| `a_split_agents_pointer_is_removed_when_it_exits` | `tddy-agent-launch/src/split_session_pointers/split_session_pointers_tests.rs` |

## Responsibility

How a split session's agent and its tools learn the remote, and which spec renders its argv.

## Boundaries

- Does **not** change the pointer format or #829's place/remove helpers — consumes them through `HostSplitSessionPointers`.
- Does **not** build the agent env allow-list — `feature/agent-tools/env-allow-list`. The resume test asserts only the session's own `TDDY_*` additions are absent.
- A split agent always runs on this host, so its pointer never takes #829's bounded ssh path.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` (#829) | `SessionPointer` with `owner` and `remote`, `place_session_pointer`, `removing_session_pointer_on_exit` | builds a split pointer and places it through those helpers | change its fields, helpers or lifecycle |
| `feature/agent-tools/host-registry` | non-optional host registry in `DaemonSessionHost` | `SplitSessions.coding_agents` from `launch_coding_agents().agents_shared()` | change how it is loaded |

## Draft PR contract

- `split_agent_spec(&CodingAgentRegistry) -> Result<CodingAgentSpec, Status>` and `SPLIT_AGENT_ID` (`agent_argv.rs`).
- `split_claude_extra_args(spec, session_dir, tddy_tools_path, tool_server_env, withdrawals)`; `tddy_tools_only_mcp_args(spec, …)` (deleted at green); `build_split_context_dir(spec, …)`; `prepare_split_agent_wiring(spec, …, withdrawals, roster_seed, …)`.
- `split_session_pointer(remote, context_dir, session_dir) -> SessionPointer`; `SplitAgentWiring.session_pointer`.
- `SplitSessionPointers` port, `SplitSessions.coding_agents`, `SplitSessions.session_pointers`; `tddy_agent_launch::split_session_pointers::HostSplitSessionPointers`.
- Every unimplemented body carries `// TODO(split-remote-env): implement`; the tests above fail on it.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** yes — #829's pointer is implemented below it
**Concurrent with:** `feature/agent-tools/agent-tool-socket-per-user`, `feature/agent-tools/acp-subagent-leftovers`
**Blocks:** `feature/agent-tools/env-allow-list`

## Decisions & Trade-offs

- Both halves of the record land together: they touch the same split wiring and the same tests.
- **Delivery — "Also write a pointer"** (user, 2026-11-07, via orchestrator AskUserQuestion): the transport goes into tddy-tools' MCP registration env, AND the split session writes a context-dir `.tddy/session.json` with #829's machinery (owner {pid, host}, remote map with the codebase session, instance, room, identity and token; 0600; git-excluded where a repo exists; removed on exit and on delete; rewritten on resume).
- **Roster seed — "Move it too"** (user, 2026-11-07, via orchestrator AskUserQuestion): `TDDY_SESSION_AGENT_ROSTER_SEED` moves to the MCP env with the transport; no `TDDY_*` at all on the split agent's process.
- **Deletions approved** (user, 2026-11-07, via orchestrator AskUserQuestion): the resume suite's `agent_env_var` test helper (deleted in the contract), and `tddy_tools_only_mcp_args` (deleted at green, with `project_home_launch_tests.rs:174` moved off it).
- **Tests approved** (user, 2026-11-07, via orchestrator AskUserQuestion): contract committed; complexity re-scored to 3.
- The registry is resolved once per call (`split_agent_spec`) and the spec passed down, rather than passing the registry into every renderer.
- #829's helpers live in tddy-agent-launch, above tddy-session-split, so the split start reaches them through the `SplitSessionPointers` port implemented there.

### Premise check against #829's landed code (pass 2, 2026-11-07)

1. `jail_launch.rs:123` already routes `TDDY_*` to the MCP env only — the leaks are the split start, split resume and sandboxed resume (State A).
2. The pointer `remote` map was not a delivery channel for split agents: they never go through `session_pointer_of`, and their Bash is withdrawn. Kept by the user's decision above as a second channel.
3. `claude_cli_spawn_steps_tests.rs:286-317` pins a project-home main's `RoleWiring` env, not split delivery — unaffected.
4. `tddy-agent-backend/tests/split_session_env_acceptance.rs` cannot reach the split wiring — the acceptance tests live in tddy-session-split and tddy-daemon instead.
5. `split_agent_spec()` had 7 production call sites, including a fallback in `build_split_context_dir` (removed).
6. The roster seed was a second `TDDY_*` on the process.
7. Six existing tests (nine assertion sites) pinned the old delivery (Testing Plan).
8. Nothing in this changeset was already done by #829.

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
