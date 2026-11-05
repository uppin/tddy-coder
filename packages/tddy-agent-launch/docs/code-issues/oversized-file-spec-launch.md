# oversized-file: spec_launch.rs

**Location:** `packages/tddy-agent-launch/src/spec_launch.rs`
**Category:** oversized-file
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **525 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.05× over**
**Thresholds breached:** length 525 > 500
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 451 → 525 | first detection — #820 crossed it: `launch_host_pty_without_inherited_tddy` with `without_inherited_tddy_variables` (the host PTY's `env -u`), `spawn_host_pty` (the shared body of both host launches), `in_a_pane_without_inherited_tddy` (the tmux pane's scrub), while `claude_template_params` left for `harness_template_params` (−15). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Move the `TDDY_*` scrubbing (`without_inherited_tddy_variables`, `in_a_pane_without_inherited_tddy`, the pane script) into a `spec_launch/tddy_env.rs` sibling with `code-restructuring` (~60 lines). n17 deletes `launch_claude_cli` and `ClaudeCliLaunch` (~90 lines) with the bespoke claude path.
