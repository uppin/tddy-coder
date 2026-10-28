# 2026-10-26 — Agents send rich notifications to their operator

**Type:** Feature

`#home-inbox` 1/8 ([#766](https://github.com/uppin/tddy-coder/pull/766)). An agent in a
daemon-launched claude-cli or cursor-cli session runs
`tddy-tools notify <title> [--body <md>|-] [--level info|success|warning|error] [--sound]`; the
daemon validates it, authenticates it like a hook, and publishes one `AttentionRequired` / `Agent`
notification carrying the title, the markdown body, the level, the sound flag and the session's web
route. Telegram renders it as HTML, chunked, silent without `--sound`, never de-duplicated. Decisions:
the Superset model (a CLI with credentials from the environment, no MCP tool, developer 2026-10-25);
`pulldown-cmark` for the markdown (developer, 2026-10-25); limits are refusals, never truncation.
Across:

- `tddy-daemon-kernel` — `session_notification::{AgentNotification, NotificationLevel}`,
  `SessionNotification.agent`, `SessionNotificationSource::Agent`; `notification_markdown`
  (`to_telegram_html`, `to_plain_text`, `escape_html`); `agent_notification_limits`.
- `tddy-service` — `ActivityService.ReportOperatorNotification`, `SessionNotificationEvent` fields
  7–11, `SESSION_NOTIFICATION_SOURCE_AGENT`, `enum NotificationLevel`;
  `session_activity::notify_env` (the five variable names, shared by the launch and the CLI).
  `activity_pb.ts` regenerated; the web reads none of the new fields.
- `tddy-session-activity` — `service/operator_notifications.rs` (session-type check, then a
  constant-time hook-token comparison that refuses a session which recorded no token);
  `ActivityPorts.daemon_instance_id` for the link; `streams::session_notification_event` maps the
  new fields. This crate's first own suite, `tests/operator_notification_acceptance.rs`.
- `tddy-session-lifecycle` — `PeerRoutedActivity` delegates the RPC unrouted;
  `ActivityPorts.daemon_instance_id` wired from `local_instance_id_for_config`.
- `tddy-tools` / `tddy-tools-cli` — `notify_cli.rs` (socket when `TDDY_DAEMON_SOCKET` is set, else
  HTTP to `TDDY_DAEMON_URL`; not fail-quiet) and the `notify` subcommand.
- `tddy-agent-launch` — `notify_env.rs` (`notify_env_pairs`, `AGENT_NOTIFY_GUIDANCE`,
  `agent_notify_launch`, `agent_notify_unless_role`, `resumed_agent_notify_launch`) applied in
  claude-cli start and resume (env + `<session dir>/agent-system-prompt.md`, the guidance composed
  with a managed recipe's prompt); cursor-cli start and resume get the env only
  (`chat::cursor_notify_env`, `chat::resumed_cursor_notify_env`).
- `tddy-telegram` — `TelegramSender::send_html_message(chat_id, html, silent)`;
  `InMemoryTelegramSender::recorded_html`. `tddy-telegram-control` — `agent_notification_telegram.rs`
  (`AgentNotificationTelegram::{render, send_to}`, 4,096-byte well-formed chunks);
  `TelegramDaemonHooks` admits `Agent` and skips the dedupe for it.

Not wired, recorded: a session with a standing role (the project-home main has no shell) gets neither
the environment nor the guidance, on start or resume — the follow-up is an MCP tool,
[`docs/dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md`](../todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md).
Sandboxed claude-cli sessions have no hook token and are not wired.

Wrap cleanups (`/pr-wrap`): the notify wiring moved from `claude_cli_spawn_steps.rs` (498 → 397
production lines, re-exported there) into `notify_env.rs`; `to_telegram_html` split (61 → 35 lines);
the start and resume replies extracted, so every function this node grew ends at or under its
base length — `spawn_claude_cli_session_inner` 310 → 309, `spawn_cursor_cli_session_reporting`
208 → 202, `resume_claude_cli_session` 142 → 141, `resume_session_at_session_coordinate` 202 → 189
(the cursor-cli branch is `LaunchSessions::resume_cursor_cli`). Added tests: a cursor-cli
notification is accepted, a session with no recorded token refuses an empty one, an unreachable
socket and `--body -` from the CLI.
