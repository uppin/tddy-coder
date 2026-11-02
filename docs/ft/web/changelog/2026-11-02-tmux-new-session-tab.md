# 2026-11-02 — Start a new tmux session as a tab

The tmux section of the **+** menu opens with **New tmux session…**. It asks for a name — prefilled
with the session's short id, or that id with `-2`, `-3`, … when a listed tmux session has it — and
Enter opens a **tmux: <name>** tab on a new tmux session in the session's worktree, run as the
session's OS user. Escape cancels. A name that already exists is attached to instead of failing;
an empty name or one containing `:` or `.` is refused. Closing the tab detaches, so the session
outlives the tab, the browser and the tddy session, and can be picked up from a real terminal.
See [Session terminal tabs](../session-terminal-tabs.md) and
[Terminal sessions § tmux terminals](../../daemon/terminal-sessions.md#tmux-terminals).
