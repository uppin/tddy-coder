# 2026-10-24 — `StartTerminalSession` takes a `kind`

`StartTerminalSessionRequest.kind` (field 4) chooses what a started terminal runs: empty or `"bash"`
is the login shell; a registered CLI harness session type (`claude-cli`, `cursor-cli`,
`opencode-cli`, `muse-cli`, `warp-cli`) starts that harness fresh and interactive in the session's
worktree, its binary resolved as for its own sessions. `ListTerminalSessions` reports the harness as
the tab's `kind`. An unknown kind is `INVALID_ARGUMENT`; a room-backed (tddy-coder) session refuses
every agent kind. Agent terminals get no session environment and are not persisted. Known limitation:
a `claude-cli` tab in a `claude-cli` session shares the worktree's hooks
([TODO](../../../dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md)). See
[Terminal sessions § Agent terminals](../terminal-sessions.md#agent-terminals).
