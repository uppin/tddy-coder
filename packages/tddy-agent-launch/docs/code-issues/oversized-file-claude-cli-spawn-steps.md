# oversized-file: claude_cli_spawn_steps.rs

**Location:** `packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs`
**Category:** oversized-file
**Detected:** 2026-11-04 by `/pr-wrap` step 3.5 file-length gate on PR #801 (`#agent-tools` 12/19)
**Metrics:** **558 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.12× over**
**Thresholds breached:** length 537 > 500
**Status:** Open — regressed 2026-11-05 by #820 (537 → 558) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-04 | 534 → 537 | first detection (already over budget at the base, `origin/feature/agent-tools/acp-agents`, with no record) — PR #801 added `ClaudeCliProcess.ssh_alias` and passes it as `ClaudeCliLaunch::remote_host`, so an ssh session's slot files are written on the ssh target. Not split: the developer deferred all code restructuring of the `#agent-tools` stack to its sweep node (2026-11-04) |
| 2026-11-05 | 537 → 558 | **worse by 21** in #820: `record_role_of_session_type_before_spawn` (the role record any CLI session type takes, shared with the generic start) and the `managed_claude_cli_launch` pieces the generic managed step reuses; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Node n14 `cli-spec-features` ([#820](https://github.com/uppin/tddy-coder/pull/820)) (`#agent-tools`) moves the claude features onto the generic CLI spec path and n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)) deletes the hand-built path; `ClaudeCliProcess` and its steps are what that replaces. If the file survives them, split the tmux-run steps (`run_in_tmux`, the pane environment file) into a sibling module with `code-restructuring`.
