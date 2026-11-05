# oversized-file: coding_agent_launcher.rs

**Location:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 by `/pr-wrap` step 3.5 file-length gate on PR #726 (`#coagents` 7/8)
**Metrics:** **685 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.37× over**
**Thresholds breached:** length 685 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 419 → 685 | first detection — PR #726 crossed it: the headless launch (`DaemonCodingAgentLauncher::headless_plan`, `bind_approvals`, `refuse_unless_daemon_user`, `tool_server_spec`, `HeadlessWiring`, `with_headless_wiring`, `with_approval_routes`) and its free helpers (`headless_program`, `headless_route_env`, `write_mcp_config`). Not split: stack-shared — `#coagents` 2/8 (#721) and 5/8 (#724) edit this file; the developer deferred code issues for this stack to its sweep node (2026-10-25) |
| 2026-10-25 | 695 → 872 | **worse** by 177 in PR #776 (`#coagents` 9/9): the three CLIs' MCP configs in `headless_plan` (`mcp_servers_wrapped`/`mcp_servers_by_name`/`write_owner_only_json`, cursor's `write_cursor_project_mcp_config` and `exclude_from_git`), the owner-tagged `claim_conversation_files` with its `ClaimedFiles` guard, and `sweep_headless_files_left_by_this_daemon`. The headless group the seam above names is now ~440 lines, so the move it proposes still takes the file under budget. Not split: the developer deferred every code issue of the `#coagents` stack past its sweep node too (2026-10-25), so this is left for after the stack lands |

## What would close it

The headless launch is one cohesive group (~265 lines) that shares only `CodingAgentLaunch`,
`Self::refuse` and `tool_server_spec` with the ACP path. Moving `headless_plan`, `bind_approvals`,
`refuse_unless_daemon_user`, `headless_program`, `headless_route_env` and `write_mcp_config` into a
`coding_agent_launcher/headless.rs` sibling (engine-driven, `code-restructuring`; the
`headless_tests.rs` module already lives there) takes the file under budget.

## Measurement update 2026-11-02 — #agent-tools 6/12 (#795)

**Regressed:** 872 → 967 production lines: `with_coding_agents`, `headless_plan_from_spec` and `headless_provider_env` (extracted from `headless_plan`). The hand-built Claude `mcp_servers_wrapped` registration stays for launchers built without coding agents. **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02).

## Measurement update 2026-11-03 — #agent-tools 9/12 (#798)

**Regressed:** 967 → 1036 production lines: every headless harness planned from its spec, and the written-file record. `write_cursor_project_mcp_config`, `mcp_servers_by_name`, `headless_program` and `OZ_BINARY` stay until their deletion is approved (`docs/dev/todo/2026-11-03-headless-others-legacy-argv-deletion.md`). Regressed by #798 (`#agent-tools` 9/12). **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-03).

## Measurement update 2026-11-04 — #agent-tools 11/12 (#800)

**Regressed:** 1036 → 1291 production lines: the ACP launches planned from their specs (`acp_plan_from_spec`, `acp_mcp_servers_from_spec`, `acp_session_meta_from_spec`, the `session_meta_for` override, the inline OpenCode route config). The hand-built ACP path (`harness_command`, `apply_route`, `claude_agent_acp_command`, `opencode_route_config`, `opencode_withdrawal_config`, the ACP constants) stays for launchers built without coding agents (`docs/dev/todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md`); deleting it is the sweep's first cut here. **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-04).

## Measurement update 2026-11-05 — #agent-tools 13/19 (#819)

**Unchanged in kind:** 1291 → 1297 production lines (+6). `route_from_vault` reads the launch's agent from the host's registry (`routed_agent_on_host`) instead of the closed `Harness` table, and takes the registry as a parameter. No cut here; the sweep's first cut stays the hand-built ACP path. **Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-03).
