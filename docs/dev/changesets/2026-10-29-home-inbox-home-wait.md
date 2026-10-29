# 2026-10-29 — WaitProjectNotifications, ReadProjectNotifications and the project notification log

**Type:** Feature

`#home-inbox` 7/9 ([#772](https://github.com/uppin/tddy-coder/pull/772)). The project-home agent
waits on a durable, cursor-addressed log of its project's attention notifications. Across:

- `tddy-session-activity` — `project_notification_log::{ProjectNotificationLog,
  ProjectNotificationRecord, ProjectNotificationScope, NotificationRead, NotificationWait,
  PROJECT_NOTIFICATION_CAPACITY, MAX_NOTIFICATION_WAIT, clamp_wait, record_json}`: a bus subscriber
  taking `AttentionRequired`, a 1000-record window per project, JSON lines under
  `<data dir>/projects/<project_id>/notifications.jsonl` replayed on start, a per-project `Notify`.
- `tddy-tool-engine` — `project_home_inbox_tools.rs` (`WaitProjectNotifications`,
  `ReadProjectNotifications`, `MAX_WAIT_MS`, `NotificationsPage`, `ProjectHomeInboxPort`,
  `NO_NOTIFICATION_LOG`); `session_follow::{page_size, parse_cursor}` made `pub`; both tools are
  target tools and read-only-allowed.
- `tddy-session-lifecycle` — `DaemonSessionHost::{with_project_notification_log,
  project_notification_log}` (`svc_project_notification_log_wiring.rs`).
- `tddy-daemon-rpc` — `exec_tool/project_home_inbox.rs`, the port over the log on `DaemonProjectHome`.
- `tddy-daemon` — `mount_host::open_shared` opens the log and registers it on the bus.
- `tddy-session-tool-client` — `remote_block_arg_keys` clamps `WaitProjectNotifications`'
  `timeout_ms`.
- `tddy-agent-launch` — the project-home prompt names the tools.

Decisions:

- **Scope struct** (`project_id`, `os_user`, `exclude_session_id`) filtered before paging, so page
  sizes hold.
- **JSON port**: tool-engine cannot name session-activity's record (cycle through
  `tddy-daemon-sandbox`), so the daemon serialises records; one serde DTO is the file line and the
  answer object.
- **Empty-page cursor is the newest `seq`**, diverging from the follow tools' "unchanged", so a wait
  "from now" never skips a record published between two polls.
- **`lost` on a wait too**, and counted over the project's records whoever made them.
- **Compaction** at twice the window, under the project lock: the temporary file is `sync_all`ed
  before the rename and the directory after it (revised at validation). A torn last line is ended
  with a line break on replay so the next append is not lost with it (added at validation).
- **A session of no project is skipped quietly** (revised at validation: it is a normal session, and
  an `Err` made the bus warn on every one); an unresolvable OS user or unreadable record is an error.
- **Waiters** `enable` their `Notified` before reading the window (made explicit at validation;
  `notify_waiters` already reached it from creation).
- **`tddy-daemon` → `tddy-session-activity`** is a direct edge to a crate already in its tree through
  `tddy-session-lifecycle`; no cycle.
- An unreadable file fails `open`, since starting empty could reuse sequence numbers.

Code issues: `exec_tool/project_home.rs` 830 → 842, `project_home_tools.rs` 734 → 750 and
`tddy-tool-engine/src/lib.rs` 887 → 888 production lines; splits deferred (stack-shared with
`#home-inbox` 8/9 and 9/9), each record carries the measurement. `svc_host_builders.rs` 499 → 500
(the new field's initialiser; the builder methods went to their own module).
`project_notification_log/storage.rs` is at 489, under budget.
