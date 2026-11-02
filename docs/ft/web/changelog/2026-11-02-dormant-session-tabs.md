# 2026-11-02 — A stopped session keeps its tabs

A session whose agent is not running keeps its tab strip. Its **Agent** tab shows the recorded
transcript the pane used to show on its own, and beside it are the terminals the daemon already runs
for it, its agent conversations, and **+**, which opens a shell or any CLI harness in the session's
worktree — no resume needed. **Add agent** on a stopped session opens a conversation tab (it used to
open nothing visible). The terminal tabs work as on a live session: full screen, the mobile shortcut
drawer, the Files tab's **Insert** and **Claim terminal**. After **Resume**, the tabs opened while the
session was stopped are still there and open conversations carry on. A session whose worktree is not
on this host is refused with a message naming it. See
[Session terminal tabs § Stopped sessions](../session-terminal-tabs.md#stopped-sessions) and
[Inactive session activities](../inactive-session-activities.md).
