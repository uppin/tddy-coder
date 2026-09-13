# 2026-09-11 — The coder's terminal coordinate serves seven of nine methods

**Category:** Future enhancement
**Source:** `#unbundle` node 6, milestone 5 — moving `tddy-coder`'s session participant onto
`terminal_session.TerminalSessionService` (changeset
[`2026-09-11-unbundle-session-io-services`](../changesets/2026-09-11-unbundle-session-io-services.md))

`tddy-terminal-rpc` serves nine methods. `tddy-coder`'s session participant registers the coordinate
but answers **seven** of them; `packages/tddy-coder/src/session_participant/terminal_session_service.rs`
refuses the other two with `Unimplemented`:

| Method | Why it is refused here |
|---|---|
| `WatchTerminalControl` | The coder's control lease is a permanent grant — it owns its own terminal and arbitrates nothing. Answering would tell **every** watching screen `you_are_controller = true`, including one the daemon's real lease has just displaced. That screen is exactly the one the event exists to correct, so a wrong answer is worse than none. |
| `StreamSessionTerminalIO` | Bidirectional terminal I/O. The coder participant already carries a session's bytes on `terminal.TerminalService/StreamTerminalIO`, which is the wire `tddy-web`'s `roomTerminalFeed` opens against it. A second bidi terminal on the same participant is net-new behaviour rather than a relocation of anything, so node 6 did not add it. |

## Why it matters

The two servers of family K now answer identically for the seven methods both serve — that is what
`packages/tddy-coder/tests/two_server_parity_acceptance.rs` asserts, and why the parity suite exists
at all (the repo has a recorded incident from the two coordinates drifting:
`packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`).

For the other two the servers are **not** equivalent, and a client cannot tell from the coordinate
which one it reached. A caller that resolves a session to the coder participant over LiveKit gets
`Unimplemented` for `WatchTerminalControl` and `StreamSessionTerminalIO`, where the same session
reached over HTTP on the owning daemon is answered. Nothing in `tddy-web` does this today — control
watching goes to the owning daemon's client (`useTerminalControl`), and LiveKit terminal bytes go to
`terminal.TerminalService` — so this is a latent asymmetry rather than a live defect.

## What closing it would take

Neither is a relocation, which is why both were left:

* `WatchTerminalControl` needs the coder to either hold a **real** lease (tracking a holder screen
  and emitting changes) or to delegate the question to the owning daemon. The first is new product
  behaviour — deciding who may drive a coder session's terminals when two screens want it; the
  second gives the coder participant a dependency on its daemon that it does not have today.
* `StreamSessionTerminalIO` needs a decision about which of the two bidi terminal wires this
  participant should serve. Adding it alongside `terminal.TerminalService/StreamTerminalIO` would
  leave two, which is the duplication node 6 is removing elsewhere; replacing it is a web change.

## Resolved while this entry was open

An earlier draft of this entry recorded that the five *unary* terminal methods
(`ClaimTerminalControl`, `StartTerminalSession`, `StopTerminalSession`, `ListTerminalSessions`,
`SendTerminalInput`) were still answered on the coder's `connection.ConnectionService` as well as on
the terminal coordinate, because `tddy-web` addressed them at this participant on the old one.

That is no longer true, and the entry is corrected rather than left to mislead: the web moved to
`terminal_session.TerminalSessionService`, the five arms were removed, `connection.proto` no longer
declares them, and `SessionConnectionServiceRpc` answers no terminal method at all. The seven-of-nine
gap above is the only part of this entry still standing.
