# 2026-09-11 — The crate serves the coordinate it owns

`terminal_session.TerminalSessionService` is served from here. `build_terminal_session_entry`
registers all nine methods over any `tddy-rpc` transport, and
`TerminalSessionServiceTonicAdapter` — generated, including the bidirectional method — serves the
same implementation over gRPC and Connect-HTTP.

The nine handlers are seven bridge calls plus an auth gate, and the two lifecycle families. Nothing
here resolves a `session_id` to a worktree, a task registry or a sandbox: a terminal's *origin*
differs per host while the streaming, replay and control semantics do not, so the hosts supply four
ports — `TerminalSessionStore`, `TerminalSession`, `TerminalRoster`, `TerminalControl`. Both
`tddy-daemon` and `tddy-coder`'s session participant register this crate's own constructor, so the
two servers of the family cannot drift; `packages/tddy-coder/tests/two_server_parity_acceptance.rs`
asserts it at the wire over the seven methods both serve.

`TerminalSession` gained a defaulted `resizable() -> bool`. It gates the post-resize drain, which
against a terminal with no PTY master — a jailed session, reached through the daemon's adapter —
would discard live bytes no replay chunk covers, leaving the client a silent gap. Additive and
defaulted, so no implementor outside the crate changed.

`login_shell` arrived: a passwd lookup with no daemon state behind it, answering a question
`StartTerminalSession` asks. Its last-resort shell is the named `DEFAULT_LOGIN_SHELL` rather than an
inline literal, so a host without it fails the spawn with the assumption named. The rest of the
daemon's `pty_runtime` stayed behind: `privilege_drop` comes from `tddy-daemon-kernel`, which
depends non-optionally on `tddy-livekit`, and depending on it from here would put the whole LiveKit
SDK inside `tddy-tools`' `--no-default-features` in-jail build, which carries none today.

`DEFAULT_INITIAL_FRAME_BYTES` (8 KiB) carries a compile-time assertion that it is nonzero and under
a megabyte. Zero would make every open replay nothing and repaint from a blank screen; a megabyte
would make the first frame of a long-lived terminal exceed a transport's per-message limit. Checked
at compile time because a default nobody passes has no test that would notice.

`into_tonic_stream` and `history_into_tonic_stream` were removed from `bridge` — no caller, and
their last plausible consumer was deleted here.

Docs: [terminal-session-service.md](../terminal-session-service.md).
Full record: [../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md](../../../../docs/dev/changesets/2026-09-11-unbundle-session-io-services.md).
