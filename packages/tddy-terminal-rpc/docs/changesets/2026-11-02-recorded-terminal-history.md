# 2026-11-02 — History of a terminal that is no longer running

**Type:** Feature

`TerminalSessionPorts` gains `recorded_history: Arc<dyn RecordedTerminalHistory>`
(`service/recorded_history.rs`, with `NoRecordedHistory`). `GetTerminalHistory` reads it only when
the store has no running terminal by that id, and pages the recording exactly as the live path pages a
capture; `None` stays `NOT_FOUND`. `pty_runtime::PtyRuntime::spawn_with_exit_hook` passes a
`tddy_pty::PtyExitHook` through. `service.rs` went 500 → 522 production lines and is recorded as
`code-issues/oversized-file-service.md` (stack-shared, not split). Tests:
`tests/recorded_history_acceptance.rs` (2). See
[terminal-session-service.md § The four ports](../terminal-session-service.md#the-four-ports). `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
