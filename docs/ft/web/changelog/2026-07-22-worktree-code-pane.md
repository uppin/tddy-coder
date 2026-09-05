# 2026-07-22 — Worktree Code pane

- Every session — terminal, workflow chat, and PR-Stack — gains a **Code** toggle that splits the main pane, opening a directory tree of the session's worktree files beside the live view (with a draggable divider). Toggling it never disturbs the running terminal or chat.
- The tree loads lazily one folder at a time and respects `.gitignore` (and hides `.git`), so build output and secrets like `.env` never appear. Selecting a file shows a read-only preview — Markdown rendered, everything else as monospace text.
- See [session-code-pane.md](../session-code-pane.md).
