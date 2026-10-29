# Native notifications

Product behaviour: [docs/ft/desktop/tddy-desktop-tauri.md § Native notifications](../../../docs/ft/desktop/tddy-desktop-tauri.md#native-notifications).

## Shape

`src-tauri/src/native_notifications.rs` holds everything that decides whether and what to show.
Everything the operating system does sits behind two ports, so tests run against in-memory fakes:

| Item | Role |
|---|---|
| `desktop_notice_for(notification, context)` | Pure. `AttentionRequired` and owned by `DesktopContext.os_user`, or `None`. Builds title, subtitle (label), body (`tddy_daemon_kernel::notification_markdown::to_plain_text(md, BODY_LIMIT = 300)` for an agent, the fixed `text` otherwise), sound (agent asked, or source `PendingQuestion`) and route |
| `SessionRoute` | `fragment()` → `/sessions/<id>?host=<instance>`, built with `url` so ids are encoded the way the dashboard reads them back; `url_within(dashboard)` swaps a page's path, query and hash for it |
| `DesktopNotice::body_with_label` | Label as the first line, then the body (the plugin has no desktop subtitle) |
| `NativeNotifier` port | `show(&DesktopNotice)`; `TauriNotifier` over `tauri-plugin-notification` 2.5+ (2.8.0 locked, on Tauri 2.12.5), `sound("default")` only when asked, and the route attached as the `extra` payload (`SessionRoute::tap_extra`) |
| `DashboardWindow` port | focus, location, `bring_to_front`, `navigate`; `TauriDashboardWindow` looks the main window up by label on every call, since it is created after the daemon |
| `open_session_route` | Bring the window forward, then navigate |
| `native_notifications/tap.rs` | The click: `TAP_ACTION_ID = "tap"`, the payload keys `EXTRA_SESSION_ID = "tddy_session_id"` and `EXTRA_HOST = "tddy_host"`, `TappedNotification {action_id, extra}`, `session_route_from_tap` (a `tap` naming both keys non-empty, else `None`) and `on_notification_tap` → `open_session_route`. Plain data, so it runs against the fake window |
| `tauri_host::register_tap_handler` | Registers the plugin's `Notification::on_action`; reads the `extra` map's string values (none when the platform reported no notification) and runs `on_notification_tap`, logging a failed open |
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
  unresolvable OS user fails the start. `start_daemon` calls `register_tap_handler` first, so a
  click is handled from the first notice. `run()` initialises the notification plugin. No webview
  capability is granted — only Rust calls the plugin.

## Platform notes

- Plugin 2.4 showed a notification on a task it spawns and drops that task's error, so only failures
  before the task starts reach `show`'s caller; not re-checked on 2.8.
- The permission API always answers "granted" on desktop; macOS silently drops refused ones.
- The `default` sound name is unverified on Linux freedesktop themes.
- **Nix on macOS:** `mac-notification-sys` compiles Objective-C against Cocoa, and the nix SDK 14.4
  headers lack `uuid_string_t`. `flake.nix` (darwin only) exports a `tddy-objc-cc` wrapper compiler
  for `.m`/`.mm` sources, per target. A global `CFLAGS` was rejected: it breaks `ring`'s `.S` files.

## Tests

- `src-tauri/tests/native_notifications_acceptance.rs` — 8 end-to-end cases over a recording notifier, 3 of them a tap mapped back to its route and opened.
- `src-tauri/tests/native_notifications_unit.rs` — 25 rule edges, including the `request_id` rule and the tap payload.
- `tddy-daemon/tests/runtime_notification_subscriber_acceptance.rs` — a registered subscriber
  receives the runtime's notifications.
- `tauri_host.rs` has no test: it needs a running Tauri application and is checked by hand (on macOS: a click brings the window forward on that session).
