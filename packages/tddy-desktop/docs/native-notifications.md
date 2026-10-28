# Native notifications

Product behaviour: [docs/ft/desktop/tddy-desktop-tauri.md § Native notifications](../../../docs/ft/desktop/tddy-desktop-tauri.md#native-notifications).

## Shape

`src-tauri/src/native_notifications.rs` holds everything that decides whether and what to show.
Everything the operating system does sits behind two ports, so tests run against in-memory fakes:

| Item | Role |
|---|---|
| `desktop_notice_for(notification, context)` | Pure. `AttentionRequired` and owned by `DesktopContext.os_user`, or `None`. Builds title, subtitle (label), body (`tddy_daemon_kernel::notification_markdown::to_plain_text(md, BODY_LIMIT = 300)` for an agent, the fixed `text` otherwise), sound (agent asked, or source `PendingQuestion`) and route |
| `SessionRoute` | `fragment()` → `/sessions/<id>?host=<instance>`, built with `url` so ids are encoded the way the dashboard reads them back; `url_within(dashboard)` swaps a page's path, query and hash for it |
| `DesktopNotice::body_with_label` | Label as the first line, then the body (plugin 2.4 has no desktop subtitle) |
| `NativeNotifier` port | `show(&DesktopNotice)`; `TauriNotifier` over `tauri-plugin-notification ~2.4`, `sound("default")` only when asked |
| `DashboardWindow` port | focus, location, `bring_to_front`, `navigate`; `TauriDashboardWindow` looks the main window up by label on every call, since it is created after the daemon |
| `open_session_route` | The click handler: bring the window forward, then navigate. **Nothing calls it** — see [the TODO](../../../docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md) |
| `DesktopNotificationSubscriber` | The bus subscriber; `wants` and `deliver` share `desktop_notice_for` |

## Suppression

`deliver` shows nothing when:

1. the window is focused **and** its location's hash equals the session's `SessionRoute::fragment`;
2. the last thing shown for that session (`last_shown`, keyed by session id) is an identical notice;
3. the notification carries a `QuestionNotice` whose `request_id` equals the one the last shown
   notice for that session was about. A status report that follows a recorded question is
   republished with that question attached and different text, and rule 2 alone shows it twice.

Only a shown notice is recorded, so one swallowed by the focused window is not "the previous one".
A poisoned map is recovered: losing a repeat check costs at most one repeated notification.

## Wiring

- `tddy-daemon`: `RuntimeOptions::with_notification_subscriber(Arc<dyn SessionNotificationSubscriber>)`;
  `mount_host::open_shared` appends each after the two built-in subscribers through a forwarding
  `RegisteredSubscriber`, because `SessionNotificationBus::with_subscriber` takes a sized subscriber.
- `lib.rs::start_daemon` builds the subscriber with `DesktopContext { os_user:
  username_for_uid(getuid()), daemon_instance_id: local_instance_id_for_config(&config) }`. An
  unresolvable OS user fails the start. `run()` initialises the notification plugin. No webview
  capability is granted — only Rust calls the plugin.

## Platform notes

- Plugin 2.4 shows a notification on a task it spawns and drops that task's error, so only failures
  before the task starts reach `show`'s caller.
- The permission API always answers "granted" on desktop; macOS silently drops refused ones.
- The `default` sound name is unverified on Linux freedesktop themes.
- **Nix on macOS:** `mac-notification-sys` compiles Objective-C against Cocoa, and the nix SDK 14.4
  headers lack `uuid_string_t`. `flake.nix` (darwin only) exports a `tddy-objc-cc` wrapper compiler
  for `.m`/`.mm` sources, per target. A global `CFLAGS` was rejected: it breaks `ring`'s `.S` files.

## Tests

- `src-tauri/tests/native_notifications_acceptance.rs` — 5 end-to-end cases over a recording notifier.
- `src-tauri/tests/native_notifications_unit.rs` — 20 rule edges, including the `request_id` rule.
- `tddy-daemon/tests/runtime_notification_subscriber_acceptance.rs` — a registered subscriber
  receives the runtime's notifications.
- `tauri_host.rs` has no test: it needs a running Tauri application and is checked by hand.
