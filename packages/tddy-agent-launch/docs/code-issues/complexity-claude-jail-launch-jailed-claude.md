# complexity: launch_jailed_claude

**Location:** `packages/tddy-agent-launch/src/claude_jail.rs:59` — `LaunchSessions::launch_jailed_claude`
**Category:** complexity
**Detected:** 2026-11-02 — `/pr-wrap` step 4 (`/analyze-clean-code`) on PR #794 (`#agent-tools` 5/12)
**Metrics:** **80 lines** · budget 60
**Thresholds breached:** length 80 > 60
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02) — **unclaimed**

## Measurement history

| Run | Metric | Note |
|---|---|---|
| 2026-11-02 | 80 lines | first detection; new in this PR |

## What the tool found

Resolves the `claude` agent and its closure, renders the slots for jail paths, prepends the pass-through args, binds the template params, and calls the launcher with a `JailSpec` and `JailedSession` built inline.

## What would close it

Extract the slot rendering (the `McpServerCommand` and `SlotInputs` block) and the `TemplateParams` binding into free functions over `ClaudeJail`.
