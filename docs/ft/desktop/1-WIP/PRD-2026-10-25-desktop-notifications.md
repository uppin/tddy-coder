# Native notifications in Tddy Desktop - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 4/8` (branch `feature/home-inbox/desktop-notify`)

## Affected Features

- **Primary**: [Tddy Desktop (Tauri)](../tddy-desktop-tauri.md) - shows operating-system notifications for its own daemon's sessions
- **Related**: [Session notifications](../../daemon/session-notifications.md) - a host that embeds a daemon can subscribe to its bus

## Summary

Tddy Desktop shows a native notification when one of its sessions needs the operator:

- **agent-written** (`#home-inbox 1/8`): the notification uses the agent's title and a plain-text
  version of its body;
- **finished** or **needs input**: the notification uses the fixed text;
- **pending question** (`#home-inbox 3/8`, once that node lands): the notification says what the session
  is asking.

Clicking one is meant to bring the window to the front and open that session. **That needs a newer
Tauri than this application is on** — see [Click](#proposed-changes) and the Out of scope list.

## Background

The application process *is* a `tddy-daemon`, yet it shows no notifications. When the operator is
in another app, a session waiting on them goes unnoticed. The notification bus has a fixed list of
subscribers, and an embedding host can't add one.

## Proposed Changes

### What's Changing

- **`RuntimeOptions::with_notification_subscriber`** lets an embedding host register a bus
  subscriber when the runtime is built.
- **The desktop subscriber.**
  - It takes `ATTENTION_REQUIRED` notifications for sessions owned by the desktop's operator.
  - It shows them through `tauri-plugin-notification` (approved 2026-10-25), with these fields:
    - **title:** the agent's title, or "`<label>` needs you";
    - **subtitle:** the session label;
    - **body:** the body stripped to plain text and cut to 300 characters;
    - **sound:** played only when the agent asked for one, or for pending questions.
  - Duplicates: the subscriber skips the notification when the window is focused and already showing
    that session. It also skips one identical to the previous notification for the same session.
- **Click.**
  - Opening a session from a click is `open_session_route`: it focuses or unminimises the main
    window and navigates it to `#/sessions/<id>?host=<instance>`.
  - **Nothing calls it yet.** `tauri-plugin-notification` 2.4, the line that builds against the
    Tauri this application is locked to (2.11), reports no clicks on desktop. Plugin 2.5 and later
    report one (`on_action`, `tap`) but need Tauri 2.12 and Rust 1.90. The developer decided
    (orchestrator, developer AFK, 2026-10-26) to ship without it and record the wiring as
    `docs/dev/todo/2026-10-26-desktop-notification-click-needs-tauri-2-12.md`.
- **Subtitle.** The plugin's desktop notification has no subtitle, so the notification shows the
  session label as the first line of its body, then the body.

### What's Staying the Same

- The web drawer's indicators and Telegram.
- A binary `tddy-daemon` is unaffected; it registers no extra subscriber.

## Impact Analysis

- **Technical.**
  - A new dependency: `tauri-plugin-notification`.
  - One `RuntimeOptions` hook in `tddy-daemon`.
  - A subscriber module in `tddy-desktop`.
- **User.** Operators working in other apps are told when a session needs them.
- **Platform.**
  - macOS and Linux.
  - Clicks are not delivered; see Click above.
  - **Known limit — permission.** On desktop the plugin's permission API always answers "granted"
    and shows no prompt. macOS asks the operator on the first notification and **silently drops**
    notifications they refuse; the desktop cannot observe that, so it neither requests permission
    nor logs a refusal.

## Acceptance Criteria

- [ ] An agent-written notification on a desktop session shows one native notification with the
  agent's title, the label and the body as plain text.
- [ ] Finished and needs-input notifications show the fixed text. Activity notifications show
  nothing.
- [ ] A notification for the session the focused window already shows is not repeated natively.
- [ ] The click handler focuses the window and opens that session's route. (Delivering a click to it
  is out of scope until Tauri 2.12.)
- [ ] Another operating-system user's session is never shown.

## Out of scope

- A click on a native notification reaching the handler (needs Tauri 2.12 and plugin 2.5+).
- Windows.
- Notification actions (reply, approve).
- Notifications from other daemons (peers).
- Browser notifications on the web dashboard.
