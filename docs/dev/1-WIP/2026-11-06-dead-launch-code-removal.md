# Changeset: Dead launch code removal

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 3 — re-scored from 4 at the contract: the bespoke ACP backends and the model-listing half were already gone, so the work is mechanical, compiler-enforced deletions across agent-launch, sandbox-runner, daemon-sandbox, sandbox-app and tddy-coder (three test-only ACP builders)
**PR**: [#832](https://github.com/uppin/tddy-coder/pull/832)

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
- **tddy-coder** — delete `acp_from_spec`'s three test-only builders; move `tests/acp_backends_from_spec_acceptance.rs` onto `spec_backend_for`.
- **tddy-integration-tests**, **tddy-acp-stub** — fix the two stale `ClaudeAcpBackend` doc comments.
- **tddy-agent-backend** — nothing left to delete: `CodexAcpBackend`/`ClaudeAcpBackend` are gone and `backend/codex_acp.rs` + `codex_acp/oauth.rs` are live (the `CodexAcpLogin` adapter reads them).

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
- [ ] runner `--model` field, `sandbox_plan_builder.rs:216` and `spawn.rs:459` writers, their tests (`params.model` at `spawn.rs:423` stays — the exec plan reads it)
- [ ] the other runner argvs that pass `--model`: `tddy-daemon-sandbox/tests/sandbox_runner_spawn_smoke.rs:218,319` (macOS, needs built binaries) and the runner's own parse tests `runner.rs:2484,2512,2539,2566`
- [ ] `tddy_coder::acp_from_spec` `claude_acp_backend_from_spec`, `codex_acp_backend_from_spec`, `opencode_acp_backend_from_spec` (only caller `tddy-coder/tests/acp_backends_from_spec_acceptance.rs:211-215`; move it onto `spec_backend_for`); stale `ClaudeAcpBackend` comments at `tddy-acp-stub/src/main.rs:1`, `tddy-integration-tests/tests/acp_backend_acceptance.rs:64`
- [ ] add `use tddy_sandbox::SandboxHandle;` to `tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs` (macOS-only compile error already on master)
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

### Contract tests (pass 2)

| Test | File:line | Status |
|---|---|---|
| `no_callerless_launch_helper_survives` | `packages/tddy-agent-launch/tests/no_callerless_launch_helpers.rs:184` | 🔴 red — lists the 16 surviving declarations (12 fns, 4 structs) |
| `the_scan_finds_a_declaration_in_each_shape_and_skips_comments_calls_and_longer_names` | same file:119 | ✅ guard — passes |
| `the_scan_reads_the_live_relaunch_steps_it_must_leave_in_place` | same file:159 | ✅ guard — passes (`relaunch_managed_workflow`, `relaunch_jail_dirs.rs` stay) |
| `the_sandbox_plan_argv_carries_no_model_flag` | `packages/tddy-daemon-sandbox/src/sandbox_plan_builder.rs:308` | 🔴 red — argv carries `"--model", ""` |
| `the_runner_refuses_a_model_flag` | `packages/tddy-sandbox-runner/src/runner.rs:2475` | 🔴 red — `--model opus` parses instead of `UnknownArgument` |

**Recorded baseline (no test):** after the ACP builder deletion, `rg 'claude_acp_backend_from_spec|codex_acp_backend_from_spec|opencode_acp_backend_from_spec' packages/tddy-coder/src` is empty.

### Baseline (pass 2, 2026-11-07, scoped)

Tests run for tddy-agent-launch, tddy-sandbox-runner, tddy-daemon-sandbox, tddy-sandbox-app; `cargo check --all-targets` for tddy-agent-backend, tddy-coder, tddy-integration-tests, tddy-acp-stub. Pre-existing failures (not this node's):
- parents' contract reds: n21 #829 — 4 agent-launch `cli_session_pointer_unit_tests`; n22 #830 — 2 sandbox-app `agent_from_spec_acceptance`; n23 #831 — 4 agent-launch `session_start_deadline`.
- binaries not built (`./test` builds them): agent-launch `clone_askpass_acceptance` (9), sandbox-app `sandboxed_codebase_seatbelt_acceptance` (16), daemon-sandbox `action_sandbox_acceptance`, `sandbox_runner_spawn_smoke`, `sandbox_session_stdio_acceptance` (1 each).
- macOS-only compile error on master: `tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs:33,37,41` — `SandboxHandle` not imported; fixed in this node at green (Scope).

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

- **2026-11-07 (user, via the orchestrator's AskUserQuestion):** delete the now-empty `tddy-agent-launch/src/jail_worktree.rs` with its `mod` line and its re-export from `svc_start_sandboxed_claude_cli_session`; remove `AcpLaunch.agent` (`tddy-coder/src/acp_from_spec.rs`), whose only readers were the three deleted builders. `packages/tddy-agent-launch/docs/module-layout.md:148` still lists the module and is updated at wrap.
- **2026-11-07 (user):** the macOS seatbelt stdio pair was investigated once: it fails identically on the parent commit (`bind egress shim: Operation not permitted`; the test's runner argv lacks `--egress-shim-port`). Both are `#[ignore]`d pointing at `docs/dev/todo/2026-11-07-seatbelt-stdio-acceptance-fails-on-macos.md`.

- An ACP backend is deleted only after a reachability check from a binary; a reachable one moves to `feature/agent-tools/acp-subagent-leftovers`.
- 2026-11-07 (user, at 6e): the three `acp_from_spec` builders count as approved unreachable leftovers — deleted, verified by the recorded baseline above, no new test.
- 2026-11-07 (user, at 6e): green also edits the extra `--model` runner argvs; `params.model` stays.
- 2026-11-07 (user, at 6e): the macOS-only `SandboxHandle` import fix lands in this node.
- A third red, `the_runner_refuses_a_model_flag`, pins that the flag is gone from the runner itself, not only from one writer.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
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
