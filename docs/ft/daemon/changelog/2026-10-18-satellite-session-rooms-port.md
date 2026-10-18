# 2026-10-18 — Closing a session's room no longer depends on LiveKit

- **Same behaviour, reached through a port.** Deleting a session, deleting it from Telegram, and
  removing its worktree still close the session's room first. A daemon with no rooms closes nothing.
