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

## What would close it

The headless launch is one cohesive group (~265 lines) that shares only `CodingAgentLaunch`,
`Self::refuse` and `tool_server_spec` with the ACP path. Moving `headless_plan`, `bind_approvals`,
`refuse_unless_daemon_user`, `headless_program`, `headless_route_env` and `write_mcp_config` into a
`coding_agent_launcher/headless.rs` sibling (engine-driven, `code-restructuring`; the
`headless_tests.rs` module already lives there) takes the file under budget.
