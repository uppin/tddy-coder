# 2026-10-28 — Native notifications for the desktop's own sessions

**Type:** Feature

PR [#769](https://github.com/uppin/tddy-coder/pull/769) (`#home-inbox` 4/8). Cross-package entry: [`docs/dev/changesets/2026-10-28-home-inbox-desktop-notify.md`](../../../../docs/dev/changesets/2026-10-28-home-inbox-desktop-notify.md)

New `src-tauri/src/native_notifications.rs` (+ `tauri_host.rs`): `desktop_notice_for`, `SessionRoute`, `DesktopNotice`, the `NativeNotifier` / `DashboardWindow` ports, `open_session_route` (not yet called) and `DesktopNotificationSubscriber` with three suppression rules (focused window on the route, identical previous notice, same pending-question `request_id`). `lib.rs` registers it in `start_daemon` and initialises `tauri-plugin-notification ~2.4`. Detail: [native-notifications.md](../native-notifications.md).
