# Changeset: Dead launch code removal

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — more than one crate (agent-launch, sandbox-runner, daemon-sandbox, sandbox-app, agent-backend, coder, integration-tests); mechanical deletions, compile + clippy gated

## Initial Discovery

[2026-11-06-dead-launch-code-removal-initial-discovery.md](./2026-11-06-dead-launch-code-removal-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-05-callerless-bespoke-launch-helpers.md](../todo/2026-11-05-callerless-bespoke-launch-helpers.md) | ✅ RESOLVED HERE |
| [2026-11-03-harness-clis-legacy-cli-harness-removal.md](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md) | ✅ RESOLVED HERE — #828 left only the `relaunch_jail_*` trio; the `hooks: unsupported` lines move to a narrowed record if still open |
| [2026-11-03-runner-model-flag-is-vestigial.md](../todo/2026-11-03-runner-model-flag-is-vestigial.md) | ✅ RESOLVED HERE |
| [2026-11-05-model-listing-and-bespoke-acp-backends-remain.md](../todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md) | ✅ RESOLVED HERE for the model-listing half and any ACP backend found unreachable; reachable remainder narrows to `feature/agent-tools/acp-subagent-leftovers` |
| [2026-11-05-cli-spec-features-leftovers.md](../todo/2026-11-05-cli-spec-features-leftovers.md) | ✅ RESOLVED HERE (record stale; its ssh-pointer part is `feature/agent-tools/session-pointer`'s) |

Deletions approved by the developer on 2026-11-06 ("Approve all").

## Affected Packages

- **tddy-agent-launch** — delete callerless helpers and the `relaunch_jail_*` trio.
- **tddy-sandbox-runner**, **tddy-daemon-sandbox**, **tddy-sandbox-app** — delete the runner `--model` field and its argv writers.
- **tddy-agent-backend**, **tddy-coder**, **tddy-integration-tests**, **tddy-acp-stub** — delete `CodexAcpBackend` / `ClaudeAcpBackend` leftovers that are unreachable.

## Related Feature Documentation

None — no behaviour changes. No PRD.

## Summary

Delete the launch code `#agent-tools` left callerless, and close the stale records that describe it.

## Scope

- [ ] `jail_launch_steps.rs` `warm_up_jail_agents`, `jail_semantic_index_env`
- [ ] `jail_worktree.rs` `project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node`, `JailBranch`
- [ ] `claude_cli_spawn.rs` `create_routed_session_dir` (move `session_provider_route_tests.rs` onto a spec helper)
- [ ] `chat.rs` `cursor_notify_env`, `resumed_cursor_notify_env`, `cursor_hook_daemon_url`
- [ ] `relaunch_jail_steps.rs` trio + `RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge` (`relaunch_jail_dirs` stays — live)
- [ ] runner `--model` field, `sandbox_plan_builder.rs:216` and `spawn.rs:459` writers, their tests
- [ ] unreachable ACP backend leftovers (`codex_acp.rs` et al.) after a reachability check
- [ ] stale records deleted or narrowed with measurements

## Technical Changes

### State A

See Initial Discovery, Exploration 3 items 4, 8, 9, 10. All helpers are `#[allow(dead_code)]` with
`TODO(cli-dispatch-collapse)` or `TODO(harness-clis)`.

### State B

None of the listed symbols exist; `rg` for each name in production `src/` returns nothing; every touched crate builds and passes clippy.

### Delta

Deletions only, plus test call sites moved onto existing spec helpers.

## Testing Plan

Compile and clippy for each touched crate, plus the scoped test runs, compared by failing-test name
against the pre-deletion baseline. One durable scan test pins that the names are gone.

## Acceptance Tests

| Test | File |
|---|---|
| `no_callerless_launch_helper_survives` (source scan over `tddy-agent-launch/src`) | `packages/tddy-agent-launch/tests/no_callerless_launch_helpers.rs` |
| `the_sandbox_plan_argv_carries_no_model_flag` | `packages/tddy-daemon-sandbox/src/sandbox_plan_builder.rs` (`#[cfg(test)]`) |

## Responsibility

Deleting dead launch code and the stale records describing it.

## Boundaries

- Does **not** delete anything with a live caller (`relaunch_jail_dirs`, live ACP paths) — those are `feature/agent-tools/acp-subagent-leftovers`'s to re-point.
- Does **not** change behaviour.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #828 | the YAML-only state that left these helpers callerless | deletes them | re-open #828 deletions |
| `feature/agent-tools/host-registry` | `sandbox-app` `choose_agent` edits | rebases over them (different file) | touch `agent_spec.rs` |

## Draft PR contract

The first push after this plan: the two acceptance tests above, failing.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes
**Concurrent with:** `feature/agent-tools/session-pointer`, `feature/agent-tools/host-registry`, `feature/agent-tools/bounded-waits`
**Blocks:** nothing

## Decisions & Trade-offs

- An ACP backend is deleted only after a reachability check from a binary; a reachable one moves to `feature/agent-tools/acp-subagent-leftovers`.

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
