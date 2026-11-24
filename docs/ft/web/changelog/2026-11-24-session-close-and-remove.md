# 2026-11-24 — Close a session from its drawer row

- An active row has an **×**, an inactive row that still holds a worktree a **trash bin**; both close
  the session on its owning host: every tab ends and every worktree goes, the record and branch stay.
- When something could be lost, the close dialog lists each worktree's files and unpushed commits,
  shows an unreadable worktree's reason with Retry, and asks whether the sessions it started close too.
- A closing row shows a spinner; closes run at once and never block the drawer, and a failed step is
  reported in the drawer notices.

Feature: [session-drawer.md § Closing a session](../session-drawer.md#closing-a-session)
