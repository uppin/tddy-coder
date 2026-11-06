# Changeset: Split sessions reach their remote through the pointer

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — four crates; consumes the pointer contract; split start, resume and sandboxed-codebase paths

## Initial Discovery

[2026-11-06-split-remote-env-via-pointer-initial-discovery.md](./2026-11-06-split-remote-env-via-pointer-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-03-split-sessions-render-the-embedded-agent-spec.md](../todo/2026-11-03-split-sessions-render-the-embedded-agent-spec.md) | ✅ RESOLVED HERE (both halves: host spec, `TDDY_REMOTE_*` off the process) |

No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-session-split** — `split_session.rs` remote env builders; `agent_argv.rs` renders from the host registry; `coding_agent_wiring.rs`.
- **tddy-agent-launch** — `jail_launch.rs:123` stops routing `TDDY_REMOTE_*` onto the process; `cli_session_pointer.rs` writes the remote map.
- **tddy-session-lifecycle** — passes the host registry to split start and resume.
- **tddy-coding-agent-specs** — `embedded_spec` loses its split-session callers (`TODO(jail-agnostic)` resolved).

## Related Feature Documentation

None — the agent sees the same remote; only the channel changes. No PRD.

## Summary

A split session's agent process gets no `TDDY_REMOTE_*`. Its tools reach the remote through the
session pointer's `remote` map and the MCP server env, and the split agent's argv is rendered from
the host's registry (operator overrides included), not the embedded spec.

## Scope

- [ ] `TDDY_REMOTE_*` written to the pointer `remote` map / MCP env only, never the agent process env
- [ ] `split_claude_extra_args` takes the host registry; split start, `SplitAgentWiring` resume and sandboxed-codebase start pass it
- [ ] `embedded_spec` has no split-session caller

## Technical Changes

### State A

- Built in `tddy-session-split/src/split_session.rs` (`split_remote_tool_env` :211, `_from_join` :183, sandboxed :226-250, called :351); delivered via `coding_agent_wiring.rs:30-55`, `tddy-agent-launch/src/jail_launch.rs:123`, the pointer `remote` map (`cli_session_pointer.rs:57`); `cli_agent_steps.rs:644-652` already routes `TDDY_*` to the MCP env only.
- `tddy-coding-agent-specs/src/lib.rs:105` `TODO(jail-agnostic)`; split callers `agent_argv.rs:7,182`, `split_session.rs:816`.
- Asserted today by `claude_cli_spawn_steps_tests.rs:286-317` and `tddy-agent-backend/tests/split_session_env_acceptance.rs` (which pin the current delivery and change with this node).

### State B

Every delivery goes through `route_tool_server_env`'s split (MCP env) or the pointer written by
`feature/agent-tools/session-pointer`. `agent_argv` renders from `registry.get(<agent>)`.

## Testing Plan

Integration tests on the split start and resume wiring with an overriding host registry; env
assertions on the spawned agent command.

## Acceptance Tests

| Test | File |
|---|---|
| `a_split_agent_process_carries_no_tddy_remote_env` | `packages/tddy-agent-backend/tests/split_session_env_acceptance.rs` |
| `a_split_agents_tools_reach_the_remote_through_the_pointer` | `packages/tddy-agent-backend/tests/split_session_env_acceptance.rs` |
| `a_split_session_renders_the_hosts_overridden_agent_spec` | `packages/tddy-session-split/src/agent_argv.rs` (`#[cfg(test)]`) |
| `a_resumed_split_session_renders_the_hosts_agent_spec` | `packages/tddy-session-split/tests/split_resume_spec.rs` |

## Responsibility

How a split session's agent and its tools learn the remote, and which spec renders its argv.

## Boundaries

- Does **not** change the pointer format — consumes `feature/agent-tools/session-pointer`'s.
- Does **not** build the agent env allow-list — `feature/agent-tools/env-allow-list`.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` | owned pointer with `remote` map, SSH write/removal | writes the split remote into it | change its fields or lifecycle |
| `feature/agent-tools/host-registry` | non-optional host registry in `DaemonSessionHost` | passes it to split start/resume | change how it is loaded |

## Draft PR contract

The first push after this plan: `split_claude_extra_args(registry, …)` signature, plus the four
acceptance tests above, failing.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — needs `feature/agent-tools/session-pointer`'s pointer behaviour (its tools resolve through it)
**Concurrent with:** `feature/agent-tools/agent-tool-socket-per-user`, `feature/agent-tools/acp-subagent-leftovers`
**Blocks:** `feature/agent-tools/env-allow-list`

## Decisions & Trade-offs

- Both halves of the record land together: they touch the same split wiring and the same tests.

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
