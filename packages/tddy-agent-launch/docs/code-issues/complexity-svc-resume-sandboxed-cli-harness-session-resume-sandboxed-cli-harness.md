# complexity: resume_sandboxed_cli_harness

**Location:** `packages/tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs:46` — `resume_sandboxed_cli_harness`
**Category:** complexity
**Detected:** 2026-11-05 — `/pr-wrap` on #819 (`#agent-tools` 13/19), fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run
**Metrics:** **74 lines**
**Thresholds breached:** length 73 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-04)
**Status:** Open — regressed 2026-11-05 by #820 (73 → 74) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 72 → 73 | first record — over the threshold before #819. **Worse by 1** in #819: `CliSessionType` is no longer `Copy` (it holds the spec's `RoutedAgent`), so `kind.clone()` wraps the `resolve_session_agent` call onto two lines |
| 2026-11-05 | 73 → 74 | **worse by 1** in #820: the resume binds the resumed conversation (`resumed_conversation`) and the node launcher; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |

## What would close it

Share the jail relaunch steps with `resume_sandboxed_claude_cli_session` in one helper.
