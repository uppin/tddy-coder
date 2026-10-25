# complexity: headless_plan

**Location:** `packages/tddy-session-lifecycle/src/coding_agent_launcher.rs` — `DaemonCodingAgentLauncher::headless_plan`
**Category:** complexity
**Detected:** 2026-10-25 by `/pr-wrap` on PR #776 (`#coagents` 9/9), hand-measured by brace matching
**Metrics:** **91 lines raw · brace depth 3**
**Thresholds breached:** length 91 > 60
**Restructure:** `extract_method` — ordinary work, no `/code-restructuring` needed
**Status:** Open — unclaimed

## Measurement history

| Run | Raw lines | Brace depth | Note |
|---|---|---|---|
| 2026-10-25 | 59 → 91 | 3 → 3 | first detection — PR #776 crossed it: the per-CLI MCP hand-off (claude and oz write an owner-only file in a tagged conversation directory, cursor writes its project config into the worktree, codex is handed the server inline) moved into one `match harness`. Not split: the developer deferred every code issue of the `#coagents` stack past its sweep node (2026-10-25) |

## What the tool found

One body does three things: refuse a launch the daemon cannot run headless, build the
`HeadlessCommand` (program, environment, working directory, model), and hand each CLI the
`tddy-tools` MCP server in that CLI's own shape — four arms, two of which also claim the
conversation's directory and one of which binds the approval route.

## Why it matters here

The order inside the claude arm is the contract item 4 of the sweep introduced: the approval route
is bound only once every file the plan writes is written, so a failing plan leaves no route. Inline
among the other arms, that ordering reads as incidental.

## What would close it

- the `match harness` → `fn hand_tool_server(&self, launch, wiring, conversation, harness, server, &mut command) -> Result<(), SubagentError>`
- optionally one helper per file-writing CLI (`claude_mcp_config`, `oz_mcp_config`), so the
  route-binding order lives in a function named for it

What remains is about 45 lines: refuse, resolve the program and environment, build the command,
hand the server.

## Verified by hand

2026-10-25 — Counted at PR #776's tip and at its base (#727, `ff19a0863`, 59 lines) by brace
matching from the `fn` line, strings and comments stripped.
