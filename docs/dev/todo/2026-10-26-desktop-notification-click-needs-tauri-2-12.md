# 2026-10-26 — A click on a Tddy Desktop notification cannot open its session until Tauri 2.12

**Category:** Future enhancement
**Source:** `#home-inbox` 4/8 desktop-notify (#769), pass 2 — the click path the PRD names as a risk

A native notification Tddy Desktop shows does nothing when clicked. The handler exists and is
tested: `tddy_desktop::native_notifications::open_session_route` brings the main window to the front
and navigates it to `#/sessions/<id>?host=<instance>`. **Nothing calls it**, because the plugin line
this application can build against reports no clicks.

| Plugin | Needs | Desktop click |
|---|---|---|
| `tauri-plugin-notification` 2.4 (what `packages/tddy-desktop/src-tauri/Cargo.toml` pins as `~2.4`) | Tauri 2.10+ — fits the locked 2.11.5 | **None.** `show()` is fire-and-forget through `notify-rust` |
| `tauri-plugin-notification` 2.5 and later | Tauri 2.12 and Rust 1.90 | `Notification::on_action`, action id `tap`, on desktop |

## Why it was not done in `#home-inbox` 4/8

Reaching the plugin's click callback means bumping Tauri from 2.11 to 2.12 and the toolchain's Rust
to 1.90 — a change to the application's whole windowing and webview stack, which a notification
feature should not carry.

## What closing it would take

Wiring is the only piece missing: after Tauri 2.12 and Rust 1.90 are on the workspace, raise the
plugin to 2.5 or later, register `app.notification().on_action(..)` in `start_daemon`'s setup, map a
`tap` on a notification the desktop showed back to its `SessionRoute`, and call
`open_session_route(&TauriDashboardWindow, &route)`. The notification's payload must carry the
session id and host so the callback can rebuild the route. Verify by hand on macOS and Linux (the
callback is reported "as far as the notification server reports it").
