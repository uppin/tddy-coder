# 2026-11-21 — Deleting a tmux-run session ends its agent

Deleting a tmux-run [session](../session-liveness.md#deleting-a-tmux-run-session) now ends the agent itself, not only its
tmux session: its pane process gets `SIGTERM`, then `SIGKILL` after a 5 s grace, each only while the pid is still
the process that started when the agent did (recorded on the roster's `tmux` node). A signal to a tool session's
direct or worker child never reaches a recycled pid. The new-tab menu's sandbox and target groups are one
component, with no visible change.
