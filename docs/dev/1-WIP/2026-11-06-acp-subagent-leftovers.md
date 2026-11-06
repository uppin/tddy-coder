# Changeset: ACP subagent leftovers

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — three crates; session lifetime of the OpenCode route file (process/ownership semantics)

## Initial Discovery

[2026-11-06-acp-subagent-leftovers-initial-discovery.md](./2026-11-06-acp-subagent-leftovers-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) | ✅ RESOLVED HERE (launcher half fixed by #825; `tddy-coding-subagent` remainder here) |
| [2026-11-04-opencode-acp-route-file-has-no-session-owner.md](../todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md) | ✅ RESOLVED HERE |
| [2026-11-05-model-listing-and-bespoke-acp-backends-remain.md](../todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md) | Claimed by `feature/agent-tools/dead-launch-code`; any reachable ACP backend it hands over is re-pointed here |

## Affected Packages

- **tddy-coding-subagent** — `session_meta_for` default no longer `withdrawal_session_meta`; `confinement.rs` constants (`CLAUDE_NATIVE_TOOLS`, `ACP_HARNESSES`, `opencode_withdrawal_config`) replaced by spec data; `catalog.rs`, `transports.rs`, `lib.rs` re-pointed; `CodingAgentSession::start` takes a hooks owner.
- **tddy-session-lifecycle** — `coding_agent_launcher.rs:385-387` writes the OpenCode route file under the session's hooks owner instead of inline `OPENCODE_CONFIG_CONTENT`.
- **tddy-coding-agent-specs** — `tests/acp_slots_unit.rs` assertions move onto the spec path.

## Related Feature Documentation

None — no user-visible change. No PRD.

## Summary

`tddy-coding-subagent` stops naming agents: its withdrawal and transport tables come from the specs,
and an ACP session owns its OpenCode route file for exactly its lifetime.

## Scope

- [ ] `session_meta_for` has no agent-specific default
- [ ] `confinement.rs` agent constants deleted; consumers read the spec
- [ ] `CodingAgentSession::start` gets a session-lifetime hooks owner; OpenCode route file written, then removed after the session
- [ ] reachable ACP backend leftovers handed over by `feature/agent-tools/dead-launch-code` re-pointed onto `SpecAcpBackend`

## Technical Changes

### State A

Exploration 3 items 6, 7, 10: `tddy-coding-subagent/src/launcher.rs:230-240` (`TODO(acp-agents)`),
`confinement.rs`, `coding_agent_launcher.rs:385-387` (`NoHooks`, inline `OPENCODE_CONFIG_CONTENT`).

### State B

Agent-specific withdrawal/native-tool data lives in the specs' ACP slots; `tddy-coding-subagent`
has no agent name in production code (extend the existing no-agent-name scan to the crate).

## Testing Plan

Integration via `tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` (route file
present during, absent after); unit tests on the spec-driven withdrawal; a source scan.

## Acceptance Tests

| Test | File |
|---|---|
| `an_opencode_acp_session_owns_its_route_file_for_its_lifetime` | `packages/tddy-session-lifecycle/tests/acp_agents_from_spec_acceptance.rs` |
| `withdrawal_comes_from_the_spec_acp_slot` | `packages/tddy-coding-agent-specs/tests/acp_slots_unit.rs` |
| `coding_subagent_names_no_agent` (source scan) | `packages/tddy-coding-subagent/tests/no_agent_names.rs` |

## Responsibility

The agent-specific tables left in `tddy-coding-subagent` and the OpenCode route file's owner.

## Boundaries

- Does **not** delete unreachable backends — `feature/agent-tools/dead-launch-code`.
- Does **not** touch the headless conversation pointer — `feature/agent-tools/session-pointer`.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/session-pointer` | `coding_agent_launcher.rs` without `forwards_server_env` | edits the same file after it | re-add server env forwarding |
| `feature/agent-tools/dead-launch-code` | unreachable ACP backends deleted; reachable list | re-points the reachable ones | delete code itself |

## Draft PR contract

The first push after this plan: the hooks-owner parameter on `CodingAgentSession::start`, plus the
three acceptance tests above, failing.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — edits `coding_agent_launcher.rs` after `feature/agent-tools/session-pointer`, and consumes `feature/agent-tools/dead-launch-code`'s reachability list
**Concurrent with:** `feature/agent-tools/split-remote-env`, `feature/agent-tools/agent-tool-socket-per-user`
**Blocks:** nothing

## Decisions & Trade-offs

- The planning-time line said this node waits on bounded-waits for a shared launcher file; discovery shows the shared file is `coding_agent_launcher.rs`, which session-pointer edits — the real edges are session-pointer and dead-launch-code.

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
