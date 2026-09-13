# 2026-09-11 — The session participant's terminal family moves to its own coordinate

A coder session participant registers three service entries built from one session service:
`connection.ConnectionService`, `terminal_session.TerminalSessionService`, and
`terminal.TerminalService`. Because the connection dispatcher matches on the **method alone** and
discards the service name, these have to be separate entries — one service answering under two names
would serve `ListExecTools` on the terminal coordinate and `StreamTerminalOutput` on the connection
one.

`session_participant/terminal_session_service.rs` registers
[`tddy-terminal-rpc`](../../../tddy-terminal-rpc/docs/terminal-session-service.md)'s own entry
constructor — the same one `tddy-daemon` registers — and supplies only the three answers that are
genuinely the coder's: which terminals it runs, who may drive them, and which OS user they belong to.
No handler is written here, so the replay model, the offsets and the ACK framing cannot diverge from
the daemon's. Four anonymous inline message converters in `session_participant/mod.rs` are deleted;
a name-based grep never saw them.

That lockstep is a correctness requirement, not tidiness. `2026-08-02-activities-tail-first-autoscroll`
records a session that "would have opened tail-first when reached over HTTP and head-first when
reached over LiveKit". `tests/two_server_parity_acceptance.rs` opens one session through both
servers and asserts the seven methods both serve answer identically — history offsets, stream mode,
control-claim outcome.

**Seven of nine.** `WatchTerminalControl` and `StreamSessionTerminalIO` are refused with
`Unimplemented`, each for a reason: the coder's control lease is a permanent self-grant, so a watch
event from it would tell every subscribing screen it is the controller — including the one the
owning daemon's real lease has just displaced, which is exactly the screen the event exists to
correct; and the participant already carries a session's bidirectional terminal bytes on
`terminal.TerminalService/StreamTerminalIO`. Recorded in
[`docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md`](../../../../docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md).

No terminal method is answered on `connection.ConnectionService` any more. Its dispatch is still a
string match, which is its own backlog item:
[`2026-09-11-the-coder-participant-dispatches-connection-rpcs-by-string.md`](../../../../docs/dev/todo/2026-09-11-the-coder-participant-dispatches-connection-rpcs-by-string.md).

Docs: [session-participant-rpc.md](../../../../docs/ft/coder/session-participant-rpc.md).
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
