# complexity: resumed_session_wiring

**Location:** `packages/tddy-agent-launch/src/cli_agent_steps/resumed_wiring.rs` — `LaunchSessions::resumed_session_wiring`
**Moved:** 2026-11-21 by #980 (`#orchestra` 2/9), from `cli_agent_steps.rs` (the file-length split)
**Category:** complexity
**Detected:** 2026-11-05 by `/pr-wrap` on PR #820 (`#agent-tools` 14/19), measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap
**Metrics:** **98 lines** (fn line to closing brace, 2026-11-24)
**Thresholds breached:** length 72 > 60 (`/analyze-clean-code`)
**Status:** Open — partially fixed 2026-11-21 by #980 (110 → 97), 98 since 2026-11-24, still over 60 — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-05 | 0 → 72 | first detection — #820: the resumed managed workflow (recipe, resume goal, `prepare_managed_workflow`), the resumed notify wiring and the resumed role. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 72 → 79 | **worse by 7** in #823 (`#agent-tools` 17/19): as `wire_session_agent`, the resumed notify pairs go to the session pointer; measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-07 | 79 → 80 | **worse by 1** in #834 (`#agent-tools` 26/28): a resume resolves the session user's agent-tool socket (`agent_tool_socket_of`) and passes it to the notify wiring and the role; measured fn line to closing brace on `origin/feature/agent-tools/split-remote-env` (`cd9026d9d`) and HEAD; no coverage run |
| 2026-11-15 | 80 → 82 | **worse by 2** in #911: the `resumed_argv_hooks` call and the `hooks` field; the logic lives in the new helper beside `claude_cli_hooks`. Brace-matched on `origin/master` (`66bb45213`) and HEAD at #911's wrap |
| 2026-11-16 | 82 → 82 | #921 (`#remote-shell` 7/7): re-measured, brace-matched on `origin/feature/remote-shell/remote-hooks` (`529e4c653`) and HEAD — unchanged; this PR's `cli_agent_steps` growth is the shell slot writer beside it |
| 2026-11-21 | unchanged | #977: `resumed_argv_hooks` renamed `resumed_hooks` (it renders every adapter's hooks on resume); the call and field are the same lines. Brace-matched on `origin/master` (`2af17f11c`) and HEAD |
| 2026-11-21 | 110 → 97 | **better by 13** in #980 (`#orchestra` 2/9): the back channel (`resumed_reports_to`, `guidance_at`, the peer-name args) went in while the session-tool-token decision moved to `resumed_session_tool_token` and the transport to `session_tool_env`; the function moved to `cli_agent_steps/resumed_wiring.rs`. brace-matched fn line to closing brace on `origin/feature/orchestra/path-reach` (`c4dace05c`) and HEAD; this counter reads 110 on the base. Still over 60 |
| 2026-11-24 | 97 → 98 | **worse by 1** in `fix/target-hooks-log-relative-path`: the back channel's `guidance_at` takes the child's target worktree (`target_worktree`, so the hook-log line names the log absolutely) instead of a bool. Brace-matched fn line to closing brace on `origin/master` (`d11cc75c1`) and HEAD; no coverage run |

## What would close it

Extract the resumed managed-workflow block into a helper with `code-restructuring` (`extract_method`), and share the role/tool-server tail with `wire_session_agent`.
