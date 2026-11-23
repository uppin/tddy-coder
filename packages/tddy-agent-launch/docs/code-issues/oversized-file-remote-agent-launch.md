# oversized-file: remote_agent_launch.rs

**Location:** `packages/tddy-agent-launch/src/remote_agent_launch.rs`
**Category:** oversized-file
**Detected:** 2026-11-23 — `/pr-wrap` step 3.5 file-length gate (#1019, `#target-tmux` 1/2); measured
to `mod tests` (an early `#[cfg(test)]` re-export at L31 stops the gate's first-`#[cfg(test)]` count)
**Metrics:** **623 production lines** · budget 500 · 1.2× over
**Restructure:** not designed — the target tmux helpers (`remote_agent_tmux_command`,
`target_login_run`, `end_target_tmux_session`) and the start judgement (`judge_the_start`,
`recorded_owner`) are the obvious seams
**Status:** Open — regressed 2026-11-23 (529 → 623 in #1019); split deferred to after `#target-tmux`
lands, because #1020 (`#target-tmux` 2/2) edits the same file — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-23 | 529 → 623 | #1019: `RemoteLaunch::tmux`, the tmux command, the tmux pid source in the start judgement, `end_target_tmux_session`. Not split: #1020 edits this file (stack rule) |

## What would close it

Move the tmux-on-target command and end into `remote_agent_launch/target_tmux.rs` (beside #1020's
`target_tmux_tab.rs`), and the start judgement into `remote_agent_launch/startup.rs`; parent under 500.
