# complexity: spawn_claude_cli_session_inner

**Location:** `packages/tddy-agent-launch/src/claude_cli_spawn.rs:40` — `spawn_claude_cli_session_inner`
**Category:** complexity
**Detected:** 2026-11-02 — `/pr-wrap` step 4 on PR #791 (`#agent-tools 2/12`), measured by structural scan
**Metrics:** **318 lines** (fn line to closing brace) · budget 60
**Thresholds breached:** length 318 > 60
**Restructure:** `extract_method` — `/code-restructuring` territory, the same shape as its sibling
`complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md`
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-02 | 318 | first detection. 309 lines at base `origin/feature/agent-tools/tool-defs` (`6bbd35ee4`), 318 at PR #791's head. The 9 lines of growth are the `coding_agents` parameter and the `match` choosing where the hooks settings are written: by the `claude` spec's hooks slot at the spawn, or by `install_claude_cli_hooks` here on the hand-built launch. The match goes when the hand-built launch does (`docs/dev/todo/2026-11-02-claude-cli-hand-built-launch-removal.md`) |
| 2026-11-05 | 328 → 329 | **worse by 1** in #819 (`#agent-tools` 13/19): the host's coding agents are passed to `create_routed_session_dir` (one argument line), which builds `RouteAgent::claude_on(coding_agents)` instead of `Harness::ClaudeCli`; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |

## What would close it

The function under 60 lines. The start's phases (worktree cut, hooks, managed launch, spawn,
metadata) are each a candidate `extract_method`. Plan the seams with `restructure anchors` once
the `#agent-tools` stack has merged, because later nodes of that stack also edit this file.
