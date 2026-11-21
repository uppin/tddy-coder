# 2026-11-21 — The Worktree tab shows a target session's worktree

For a session whose agent runs on an ssh host or a Coder workspace, the Worktree tab now finds its
worktree on the target instead of showing it as missing: branch, diff summary and size are measured
there, without holding up the local rows. An unreachable target shows the row stale, and **Refresh**
measures it on the target again. See
[session-worktree-inspector.md § On a target session](../session-worktree-inspector.md#on-a-target-session).
