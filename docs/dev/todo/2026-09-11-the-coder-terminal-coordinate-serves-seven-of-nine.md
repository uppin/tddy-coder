# 2026-09-11 — The coder's terminal coordinate serves eight of nine methods

**Category:** Future enhancement
**Source:** `#unbundle` node 6, milestone 5 — moving `tddy-coder`'s session participant onto
`terminal_session.TerminalSessionService` (changeset
[`2026-09-11-unbundle-session-io-services`](../changesets/2026-09-11-unbundle-session-io-services.md))

`tddy-terminal-rpc` serves nine methods. `tddy-coder`'s session participant registers the coordinate
but answers **eight** of them; `packages/tddy-coder/src/session_participant/terminal_session_service.rs`
refuses the ninth with `Unimplemented`:

| Method | Why it is refused here |
|---|---|
| `WatchTerminalControl` | The coder's control lease is a permanent grant — it owns its own terminal and arbitrates nothing. Answering would tell **every** watching screen `you_are_controller = true`, including one the daemon's real lease has just displaced. That screen is exactly the one the event exists to correct, so a wrong answer is worse than none. |

## Why it matters

The two servers of family K answer identically for the eight methods both serve — that is what
`packages/tddy-coder/tests/two_server_parity_acceptance.rs` asserts, and why the parity suite exists
at all (the repo has a recorded incident from the two coordinates drifting:
`packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`).

For `WatchTerminalControl` the servers are **not** equivalent, and a client cannot tell from the
coordinate which one it reached. A caller that resolves a session to the coder participant over
LiveKit gets `Unimplemented`, where the same session reached on the owning daemon is answered.
Nothing in `tddy-web` does this today — control watching goes to the owning daemon's client
(`useTerminalControl`) — so this is a latent asymmetry rather than a live defect.

## What closing it would take

`WatchTerminalControl` needs the coder to either hold a **real** lease (tracking a holder screen and
emitting changes) or to delegate the question to the owning daemon. The first is new product
behaviour — deciding who may drive a coder session's terminals when two screens want it; the second
gives the coder participant a dependency on its daemon that it does not have today.

## Resolved while this entry was open

An earlier draft of this entry recorded that the five *unary* terminal methods
(`ClaimTerminalControl`, `StartTerminalSession`, `StopTerminalSession`, `ListTerminalSessions`,
`SendTerminalInput`) were still answered on the coder's `connection.ConnectionService` as well as on
the terminal coordinate, because `tddy-web` addressed them at this participant on the old one.

That is no longer true, and the entry is corrected rather than left to mislead: the web moved to
`terminal_session.TerminalSessionService`, the five arms were removed, `connection.proto` no longer
declares them, and `SessionConnectionServiceRpc` answers no terminal method at all.

`StreamSessionTerminalIO` was the second refused method until #760 (`#term-snapshot` 1/4): the web
opens every host terminal on it with a screen snapshot, so the coder serves it through the shared
bridge, and the parity suite covers it
(`both_wirings_open_one_terminal_with_the_same_snapshot_over_the_bidi_stream`).
