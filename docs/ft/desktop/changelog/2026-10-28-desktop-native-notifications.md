# 2026-10-28 — Tddy Desktop shows native notifications for its sessions

- **A session that needs the operator raises an operating-system notification**: the agent's own
  title and plain-text body, or the daemon's fixed text, with the session label as the first line.
- **Sound** plays only when the agent asked for one or the session is asking a question.
- **No repeats**: nothing is shown for the session the focused window already shows, for a notice
  identical to the previous one, or for a question already shown.
- **Only the operator's own sessions**: another OS user's sessions are never shown.
- **Clicking a notification does nothing yet**: it needs Tauri 2.12.

See [tddy-desktop-tauri.md § Native notifications](../tddy-desktop-tauri.md#native-notifications).
