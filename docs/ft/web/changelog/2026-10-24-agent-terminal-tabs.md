# 2026-10-24 — Agent terminal tabs: any CLI harness beside bash

- The terminal tab strip's `+` opens a menu: **Bash**, then every CLI harness (Claude CLI, Cursor CLI,
  OpenCode, Muse Code, Warp). Choosing a harness opens an **agent tab** — that harness running
  interactively in the session's worktree beside the main agent. A tool session offers Bash only.
  Before the session's type is known every harness is offered, and a kind the host does not run is
  refused in `sessions-terminal-error`. Escape or a click outside closes the menu. See
  [Session terminal tabs](../session-terminal-tabs.md#ux).
- Tabs are labelled by kind with a per-kind ordinal — **Bash 1**, **Warp 1** — and keep those labels
  on reattach. A bash tab's label is no longer its terminal id.
