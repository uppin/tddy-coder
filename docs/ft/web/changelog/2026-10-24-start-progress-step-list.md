# 2026-10-24 — The create pane lists each start step and a live clone bar

While a session is created, the pane lists the steps of the start under **Creating session on <host>**:
cloning the repository, the worktree, the semantic index and the agent, each marked in progress, done or
failed. The clone row shows git's stage and percent, a progress bar, and the bytes received with the
throughput. A failed start leaves the list showing the step it stopped at.

See [session-drawer.md](../session-drawer.md#start-progress).
