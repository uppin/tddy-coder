# oversized-file: opencode_cli_spawn.rs

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn.rs`
**Category:** oversized-file
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **685 production lines** (counted to the first `#[cfg(test)]`) · budget 500 · **1.37× over**
**Thresholds breached:** length 632 > 500
**Status:** Open — regressed 2026-11-06 by #823 (632 → 685) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-05 | 477 → 632 | first detection — #820 crossed it: the generic start and resume serve claude's and cursor's needs (`CliHarnessStart` gains the permission mode, ssh host, managed and role inputs; the session wiring, conversation, ssh slot writes and node launcher at the call sites; the extra `.session.yaml` fields). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 632 → 685 | **worse by 53** in #823 (`#agent-tools` 17/19): `CliHarnessStart.renderer_env`, the resume's conversation adoption, the session pointer at start and resume, and the split half of the resume (`resume_split_wiring`). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
**Better by 12:** 685 → 673 production lines (counted to the first `#[cfg(test)]` on `origin/feature/agent-tools/telegram-spec-launch` and at #825's wrap). `CliHarnessStart.renderer_env` and `harness_binary`'s `cli_harnesses.<type>.binary_path` step are deleted with the daemon.yaml agent sections. Still over budget; the split above stands. **Status:** Open — deferred to the #agent-tools sweep node (n20).

## What would close it

Split the resume (`LaunchSessions::resume_cli_harness`, `resume_cli_harness_session`) into an `opencode_cli_spawn/resume.rs` sibling with `code-restructuring`, and extract the start's metadata write and LiveKit exposure into helpers; together with n17's dispatch collapse (which renames this module the generic CLI launch) that brings it under budget.
