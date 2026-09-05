# 2026-07-19 — Workflows spawn a child conversation as a session tab

- A managed workflow (first: **grill-me** after its Create-plan phase) can hand off to a fresh implementation agent by spawning a new interactive conversation on its own git worktree; the new conversation appears as a **tab inside the parent session**, beside the Agent tab and any bash tabs. See [session-terminal-tabs.md](../session-terminal-tabs.md).
- Child tabs are discovered from the existing session list (no new RPC): any session whose `orchestrator_session_id` points at the open session renders as `sessions-child-tab-<id>`; selecting it attaches and shows that child session's live pane. A session with no children shows only the Agent tab.
