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

Clicking one brings the window to the front and opens that session.

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
- **Click.** Clicking focuses or unminimises the main window and navigates it to
  `#/sessions/<id>?host=<instance>`.
- **Permission.** The first notification asks the operating system for permission. If the operator
  refuses, nothing is shown and the refusal is logged once.

### What's Staying the Same

- The web drawer's indicators and Telegram.
- A binary `tddy-daemon` is unaffected; it registers no extra subscriber.

## Impact Analysis

- **Technical.**
  - A new dependency: `tauri-plugin-notification`.
  - A plugin permission in `capabilities/default.json`.
  - One `RuntimeOptions` hook in `tddy-daemon`.
  - A subscriber module in `tddy-desktop`.
- **User.** Operators working in other apps are told when a session needs them.
- **Platform.**
  - macOS and Linux.
  - Whether a click can be delivered depends on the platform. **Risk:** the plugin's click event on
    desktop platforms must be verified in pass 2. If it can't be delivered, the developer decides the
    fallback.

## Acceptance Criteria

- [ ] An agent-written notification on a desktop session shows one native notification with the
  agent's title, the label and the body as plain text.
- [ ] Finished and needs-input notifications show the fixed text. Activity notifications show
  nothing.
- [ ] A notification for the session the focused window already shows is not repeated natively.
- [ ] Clicking a notification focuses the window and opens that session's route.
- [ ] Another operating-system user's session is never shown.

## Out of scope

- Windows.
- Notification actions (reply, approve).
- Notifications from other daemons (peers).
- Browser notifications on the web dashboard.
