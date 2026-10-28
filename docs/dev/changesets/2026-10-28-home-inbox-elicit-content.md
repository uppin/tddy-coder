# 2026-10-28 — What a claude-cli session is asking

**Type:** Feature

`#home-inbox` 3/8 ([#768](https://github.com/uppin/tddy-coder/pull/768)). A claude-cli session's
permission prompt or `AskUserQuestion` reaches the daemon with its content, extending
`#start-parity` 6/9's pending-question record rather than adding a second store. Across:

- `tddy-agent-backend` — `build_claude_hooks_settings` installs 8 events: `PermissionRequest`, and
  `PreToolUse` with the matcher `AskUserQuestion` only.
- `tddy-changeset` — `PendingQuestion` gains `request_id`, `tool_name`, `tool_input_json`,
  `questions: Vec<PendingQuestionItem>` and `at_unix_ms` (all `#[serde(default)]`) and `summary()`
  (`Bash` → `command`; `Edit`/`Write`/`Read` → `file_path`; else compact JSON; a question → its
  first question). `HookEvent` gains `tool_name`/`tool_input` (and loses `Eq`);
  `activity_status_from_hook_with_tool` maps both new events to `WaitingForInput`.
  `ASK_USER_QUESTION_TOOL`, `hook_event_asks_a_question` and `set_pending_elicitation` (writes only
  on change) are new public surface.
- `tddy-service` — `ReportSessionStatusRequest.{hook_event_name = 6, tool_name = 7,
  tool_input_json = 8}`, `ReportSessionStatusResponse.request_id = 2` (`ok` holds 1),
  `SessionNotificationEvent.question = 12` (`QuestionNotice`), `SOURCE_PENDING_QUESTION = 5`.
- `tddy-tools` — `session_status_request` sends the event, tool and input for the two events and no
  message. Neither hook of an `AskUserQuestion` is reported as a tool call (`agent_activity_request`
  → `None`): the `PreToolUse` would leave a pending call a declined question never completes, and the
  `PostToolUse` would append an unpaired `completed` row under a fresh id that nothing reads.
- `tddy-session-activity` — `hook_reports.rs`: the question is built (and a malformed one refused
  as `InvalidArgument`) before any write; `record_pending_question` records, keeps (the follow-up
  `Notification`, filling only an empty `question`) or clears, mirroring `pending_elicitation`;
  `PendingChange::notification` publishes one `PendingQuestion` notification for a recorded question
  and the status notification carrying the notice for a kept one. New `pending_question_record`
  (`pending_question_for_hook`) and `pending_question_notice` (`question_notice_for`, a free function
  because the orphan rule refuses `From`).
- `tddy-daemon-kernel` — `QuestionNotice` (a mirror: the kernel does not depend on
  `tddy-changeset`), `SessionNotification.question`, `SessionNotificationSource::PendingQuestion`.
- `tddy-telegram-control` — new `question_alert.rs` (`QuestionAlert::{render, html, messages}`,
  local `&`/`<`/`>` escaper). `TelegramDaemonHooks` admits `PendingQuestion`, declines an
  `ActivityStatus` notification carrying a `question`, and alerts once per `request_id` (key
  `question:<id>` in `LastDeliveredPerSession`). A long question set is split with
  `agent_notification_telegram::split_html`, now `pub(crate)`; a chat that refuses one message gets
  none of the rest.

Decisions taken unattended (developer AFK), for review:

- **Orphan `completed` row dropped** at wrap: the `PostToolUse` of `AskUserQuestion` is no longer sent
  as `ReportAgentActivity` (no reader of such rows exists; `ReportSessionStatus` still clears the
  question on it). Pinned by `a_post_tool_use_of_ask_user_question_is_not_reported_as_a_tool_call`.
  The `AgentToolCall` "working" indicator for that one hook goes with it; the status report covers it.
- **Question alert split** at wrap, pinned by
  `a_question_set_over_the_telegram_limit_is_sent_as_several_well_formed_messages`.
- `ReportSessionStatus` for a kept follow-up answers an empty `request_id`; drawer texts read
  `🔔 Session {label} wants to run {tool}: {summary}` / `… asks: {first question}`.

Wrap refactor: `report_session_status_impl` 103 → ~65 lines by moving the question build
(`question_asked_by`) and the publish match (`PendingChange::{notification, recorded_request_id}`)
out of it. Code issues: no file this PR touched is over the 500-line budget (largest:
`session_notifications.rs` 394 → 429, `hook_reports.rs` 234 → ~340 production lines);
`session_status_request` grew 24 → 54 lines, under the 60-line record threshold, and is left in place
because `#home-inbox` 6/8 and 8/8 edit `session_hook.rs`.
