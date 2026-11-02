# 2026-11-02 — Work that finishes before a terminal leaves the registry: `PtyExitHook`

**Type:** Feature

`PtyRuntime::spawn_with_exit_hook(.., Option<PtyExitHook>)` runs the hook with the task id once the
process has exited or been cancelled, **before** the terminal's control is removed from the
`PtyRegistry` — so whoever finds the terminal absent can rely on the work being done. `spawn` is
`spawn_with_exit_hook(.., None)`; the runtime knows nothing of what the hook does
(`tddy-terminal-rpc`'s `pty_runtime` passes it through). `tddy-cli-sessions` writes the main
terminal's final recording with it. `#session-tabs` 2/7, cross-package entry `docs/dev/changesets/2026-11-02-session-tabs-agent-history.md`.
