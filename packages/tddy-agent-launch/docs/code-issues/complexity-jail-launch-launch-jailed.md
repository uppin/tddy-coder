# complexity: launch_jailed

**Location:** `packages/tddy-agent-launch/src/jail_launch.rs:129` — `LaunchSessions::launch_jailed`
**Category:** complexity
**Detected:** 2026-11-02 — `/pr-wrap` step 4 (`/analyze-clean-code`) on PR #794 (`#agent-tools` 5/12)
**Metrics:** **147 lines** (fn line to closing brace) · 5 parameters + `self` · budget 60 lines, 5 parameters
**Thresholds breached:** length 147 > 60
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02) — **unclaimed**

## Measurement history

| Run | Metric | Note |
|---|---|---|
| 2026-11-02 | 147 lines | first detection; new in this PR |

## What the tool found

The shared jail launcher, added by #794. One body records the role, lowers the closure (`lower_tool_jail`), picks the shim port, writes the exec plan, builds the runner argv and spawn, spawns, waits for ready, dials and bridges, and registers the session state. Each step is a few lines, but the body is their sequence, and it is the function #796 and #797 will call.

## What would close it

Three cuts along the existing steps: the spawn (argv → `SandboxRunnerSpawn` → `spawn_sandbox_plan`), the ready wait with its egress-log error, and the bridge-and-register tail. Each takes the `JailedSession` fields it reads, so no new parameter appears.
