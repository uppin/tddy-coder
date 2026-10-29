# 2026-10-29 — #home-inbox sweep: spawner read scope, NotifyOperator, desktop click, two splits

**Type:** Sweep (feature + refactor + fix)

`#home-inbox` 11/11 ([#788](https://github.com/uppin/tddy-coder/pull/788)), the stack's top node.
Across:

- `tddy-tool-engine` — `is_conversation_spawner_scoped_tool` (`ReadSessionActivity`), joined to the
  spawner's offered catalogue; new `project_home_notify_tools` (`NOTIFY_OPERATOR`,
  `NotifyOperatorArgs::into_notice`, `OperatorNotice`, `ProjectHomeNotifyPort`,
  `execute_project_home_notify_tool`), added to `is_project_home_tool`, the offered catalogue and
  the main's pre-approved tools. Declares `tddy-daemon-kernel` (already in its tree through
  `tddy-task`) for `agent_notification_limits`.
- `tddy-daemon-rpc` — `exec_tool/project_home/caller.rs`: `project_home_caller` admits a spawner's
  scoped call and returns the role it read; `refuse_a_spawner_outside_its_own_sessions` takes that
  role. `notify.rs`: `impl ProjectHomeNotifyPort for DaemonProjectHome` over
  `ProjectHomeHost.activity` (`PeerRoutedActivity`).
- `tddy-session-activity` — `answers::clear_answered_pending_question`, called by both
  `answer_permission` and `answer_question`.
- `tddy-agent-launch` — the project-home prompt names `NotifyOperator`.
- `tddy-tools` — the admission pin lists `NotifyOperator`.
- `tddy-desktop` — Tauri 2.11.5 → 2.12.5 (tauri-runtime 2.12, tauri-build/codegen/macros 2.7.4,
  tauri-utils 2.10.2, wry 0.55.1 → 0.57.1, tao 0.35.3 → 0.37.2, muda 0.20, tray-icon 0.25);
  `tauri-plugin-notification` `~2.4` → `"2.5"`, locked at 2.8.0. `native_notifications/tap.rs` and
  `tauri_host::register_tap_handler`, registered in `start_daemon`; `TauriNotifier::show` attaches
  `SessionRoute::tap_extra`.

Decisions:

- **NotifyOperator's credential** is the session's own `.session.yaml` `hook_token`, presented by
  the daemon to `ReportOperatorNotification` in process; the standing-role launch gets no notify
  environment. The tool takes no session argument: it publishes for the caller the exec-tool path
  authenticated. The session type and a non-empty token are checked first, so the activity
  service's raw `session_type` error never reaches the agent.
- **Spawner scope reads the role once** (hardened at validation): the first draft re-read the
  caller's record in the scope check and passed the call when that second read failed. The role now
  travels from `project_home_caller`, and a target with no readable changeset is refused (new test
  `a_grill_me_spawner_reading_a_child_whose_changeset_is_unreadable_is_refused`).
- **Spawner scope is this host only.** A forwarded call is refused by the peer, which confirms only
  project-home callers. Kept open:
  `docs/dev/todo/2026-10-29-spawner-read-scope-is-same-host-only.md`.
- **Race test seam.** The interleave inside `answer_permission` cannot be staged deterministically,
  so the race is tested on `clear_answered_pending_question` directly.
- **Desktop tap** reads the plugin's `extra` map as strings; a non-`tap` action, or a payload missing
  either key, opens nothing; a failed open is logged. The navigated URL keeps the dashboard's own
  origin (`SessionRoute::url_within`), so a stale payload can only name a session route.

Splits (pure moves by the restructure engine; `restructure verify --against HEAD` accounted for every
statement; hand edits limited to unused imports):

| File | Production lines before | After |
|---|---:|---|
| `tddy-daemon-rpc/src/exec_tool/project_home.rs` | 992 | 347 (+ `caller` 122, `listing` 54, `port` 280, `terminal` 102, `notify` 50, `answer` 91) |
| `tddy-tool-engine/src/project_home_tools.rs` | 771 | 485 (+ `project_home_catalogue` 239, `project_home_listing` 30, `project_home_target_host` 61) |

Every moved public item of `project_home_tools` is re-exported by name, so no public path changed.
`port.rs` holds the whole `impl ProjectHomePort for DaemonProjectHome` (a trait impl cannot be split
across files); at 280 lines it is under budget and kept whole.

Code issues closed (final measurement above; records deleted):

- `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-exec-tool-project-home.md` — 992 → 347.
- `packages/tddy-tool-engine/docs/code-issues/oversized-file-project-home-tools.md` — 771 → 485.

Backlog entries resolved and deleted:

- `docs/dev/todo/2026-10-29-conversation-spawner-cannot-read-its-childs-permission-request.md` — a
  spawner could answer its child's prompt but not read its `request_id`; resolved by the scoped
  `ReadSessionActivity` (the developer chose a read tool, 2026-10-29).
- `docs/dev/todo/2026-10-26-shell-less-sessions-cannot-notify-their-operator.md` — the project-home
  main had no way to notify; resolved by `NotifyOperator`.
- `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md` — a desktop notification
  click did nothing; resolved by the Tauri 2.12.5 / plugin 2.8.0 upgrade (approved 2026-10-29) and
  the tap handler.

Not in scope, unchanged: `tddy-tool-engine/src/lib.rs` (885 → 888 over the stack) and
`tddy-task/src/terminal_capture.rs` (712, below its base of 728) keep their records.

Manual check left for the operator: on macOS, clicking a notification brings the window forward on
that session.
