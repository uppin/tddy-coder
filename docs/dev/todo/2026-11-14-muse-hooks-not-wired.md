# 2026-11-14 — A Muse Code session reports no activity, question, status or notification

**Category:** Known defect
**Source:** project-home orchestrator driving `muse-cli` session `01a1d89b-0e66-7880-bed2-a6bec7ba7238`
(worktree `.worktrees/test-muse-drive-questions`) on 2026-11-14

## Symptoms

While Muse waited in its `request_user_input` selection UI (two questions):

- `ReadSessionActivity` answered `items: []`, `pending_question: null`.
- `WaitProjectNotifications` delivered nothing: no "waiting for input", no question record.
- `ListProjectSessions` showed `agent_status: "active"`, `pending_elicitation: false` throughout.

The orchestrator had to answer over `ClaimSessionTerminal` + `SendSessionKeys`. That path works.

## Root cause

All of these surfaces are fed by one mechanism: the agent's lifecycle hooks running
`tddy-tools session-hook` → `ReportSessionStatus` / `ReportAgentActivity`. Nothing else writes them
for a PTY session, and `muse.yaml` declares `hooks: { adapter: unsupported }`, `capabilities: []`.
The session directory accordingly holds no `agent-activity.jsonl` or `pending-question.json`, and its
`.session.yaml` has no `activity_status` (`updated_at` = `created_at`).

| Surface | Reads | Written only by |
|---|---|---|
| `ReadSessionActivity` | `agent-activity.jsonl`, `pending-question.json` (`packages/tddy-daemon-rpc/src/exec_tool/project_home_session_follow.rs:22-35`) | `ReportAgentActivity` / `ReportSessionStatus` (`packages/tddy-session-activity/src/service/hook_reports.rs:106-142`, `:226-319`), the coder presenter, sandbox and headless conversations |
| `WaitProjectNotifications` | the project log of `AttentionRequired` notifications (`packages/tddy-session-activity/src/project_notification_log.rs`) | the status hook (`session_notifications.rs:236-310`), the coder presenter, `tddy-tools notify` |
| `ListProjectSessions.agent_status` | `activity_status`, **falling back to the lifecycle `status`** (`packages/tddy-daemon-rpc/src/exec_tool/project_home/port.rs:71-75`), hence `"active"` | `update_activity_status` / `set_pending_elicitation` (`packages/tddy-changeset/src/session_metadata.rs:313-340`), called from the status hook |

Hooks are installed only for a spec with `Capability::Hooks` (or `OperatorNotify`)
(`packages/tddy-agent-launch/src/cli_agent_steps.rs:193-194`, `:405`), only in Claude's
`.claude/settings.local.json` shape (`packages/tddy-agent-backend/src/claude_hooks.rs:84-130`), and a
report from a session without the capability is refused (`hook_reports.rs:176-180`).

## The premise behind `hooks: unsupported` is wrong

Muse Code 1.4.12 (`/opt/homebrew/Caskroom/muse-code/1.4.12-R6909.1/muse-aarch64-macos`) has a
Claude-compatible hook system:

- Events (binary strings): `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`,
  `PostToolUseFailure`, `PermissionRequest`, `Notification`, `Stop`, `StopFailure`, `SessionEnd`,
  `SubagentStart`/`Stop`, `PreCompact`/`PostCompact`, `Elicitation`/`ElicitationResult`.
- Payload fields `hook_event_name`, `tool_name`, `tool_input`, `tool_use_id`; output
  `hookSpecificOutput`, `permissionDecision`, `decision.behavior`.
- `Notification` carries `notification_type` = `permission_prompt` ("Muse Code needs approval") or
  **`question_prompt`** ("Muse Code needs your input"). An embedded integration note says it
  dispatches only after a 6s deadline with a still-pending recheck.
- Configured by settings `hooks`, `managed_hooks_path` (+ `managed_hooks_env_vars`), project
  `.muse/hooks.json`, plugin `hooks/hooks.json`. `-c KEY=VALUE` sets one setting for a run.
- Observed live: this session ran the Superset hooks named by `managed_hooks_path` in
  `~/.config/muse/settings.json` (`SessionStart`, `UserPromptSubmit`, `Stop`; Muse session log seq
  18-21, 114-115).

## What remains

1. **Probe first** (~15 min, no tddy change). In a scratch worktree, give `muse` a hook on every event
   that logs its stdin (via `-c hooks=…` and via `.muse/hooks.json`), ask it to use
   `request_user_input`, and record which events fire and with what payload. Settle:
   - Does `PreToolUse` fire for the built-in `request_user_input`?
   - Are project `.muse/hooks.json` hooks loaded before workspace trust?
   - Does `-c hooks=…` install hooks without a file, and does it keep the user's
     `managed_hooks_path` hooks (Superset's) running?
2. A `muse-hooks` adapter writing the same events `build_claude_hooks_settings` does, in Muse's
   format, at the place the probe says works (prefer `-c hooks=…`: no file the agent can commit).
   Add `hooks` to `muse.yaml`'s `capabilities`.
3. Teach the shared mapping Muse's words: `notification_type: question_prompt` → `WaitingForInput` in
   `activity_status_from_hook_with_tool` (`packages/tddy-changeset/src/session_activity.rs:115-120`
   knows only Claude's `permission_prompt` / `elicitation_dialog` / `idle_prompt`), and
   `request_user_input` alongside `AskUserQuestion` in `hook_event_asks_a_question`. Its input is
   `{"questions":[{"id","header","question","options":[{"label","description"}]}]}`: Claude's shape
   plus `id`, without `multiSelect`.
4. The `WaitingForInput` / `Done` notification texts say "Claude Code" whatever the agent
   (`session_notifications.rs:246,251`). Use the spec's `product`.

Not recommended: a PTY-screen question detector. It breaks across Muse releases and themes and has
no question ids.

Answering through `AnswerSessionQuestion` is a separate item:
[2026-11-14-muse-answer-session-question.md](2026-11-14-muse-answer-session-question.md).

**Why deferred:** found by investigation only. The fix's shape depends on the probe above.
