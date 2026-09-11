# 2026-09-11 — A jailed session's capture replays through the terminal bridge

`TerminalCapture` is unchanged. What changed is its second consumer: a jailed session's own
`Arc<Mutex<TerminalCapture>>`, held by `tddy-daemon-sandbox`'s `sandbox_session.rs` because that
PTY's output arrives over a stdio bridge rather than through a `TaskChannel`, is bound to
`tddy_terminal_rpc::session::TerminalSession` by `tddy-daemon`'s `terminal_session_adapter.rs`.

So a jailed terminal's replay, prologue and absolute offsets come from
[`tddy_terminal_rpc::bridge`](../../../tddy-terminal-rpc/docs/terminal-session-service.md#the-replay-contract)
— the same arm every other attach path uses — rather than from a chunking helper of its own.

Docs: [terminal-capture.md](../terminal-capture.md) § Users outside `TaskChannel`.
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
