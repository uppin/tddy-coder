# 2026-11-02 — The daemon serves a stopped session's main terminal from its recording

**Type:** Feature

`session_dir_recorded_history.rs`: `SessionDirRecordedHistory`, the daemon's
`RecordedTerminalHistory`, reads `<data dir>/sessions/<id>/terminals/main.capture` for `main` (any
other terminal, a missing file or an unreadable one is `None`); `DaemonSessionHost`'s terminal ports
pass it. Tests: `agent_terminal_history_recording_acceptance` (6) and
`agent_terminal_recording_launch_acceptance` (2: a started and a resumed claude-cli session record
their main terminal). See [module-layout.md](../module-layout.md) and
[Terminal sessions § History of a stopped agent terminal](../../../../docs/ft/daemon/terminal-sessions.md#history-of-a-stopped-agent-terminal). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
