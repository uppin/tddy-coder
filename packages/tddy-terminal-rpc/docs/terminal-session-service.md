# `terminal_session.TerminalSessionService` (tddy-terminal-rpc)

The nine RPCs that carry a terminal, and the one implementation behind them. The proto is
`packages/tddy-terminal-rpc/proto/terminal_session.proto`; the service is `src/service.rs`; the
streaming logic every method shares is `src/bridge.rs`.

Product contract: [terminal-sessions.md](../../../docs/ft/daemon/terminal-sessions.md).

## The surface

| RPC | Shape | What it does |
|---|---|---|
| `StreamSessionTerminalIO` | bidi | Raw terminal I/O for transports that can send a streaming request body. The first client message authenticates and names the terminal; later messages carry stdin bytes. |
| `StreamTerminalOutput` | server stream | Attach to a terminal's output: mode prologue, anchored replay, then the live tail. |
| `SendTerminalInput` | unary | One write to a terminal's stdin, carrying a cumulative `input_offset`. |
| `GetTerminalHistory` | server stream | Bytes older than what a stream replayed, addressed by absolute offsets. |
| `StartTerminalSession` | unary | Spawn a Bash tool in the session's checkout and return its `terminal_id`. |
| `StopTerminalSession` | unary | Terminate one tool and deregister it. |
| `ListTerminalSessions` | unary | The tools running in a session. |
| `ClaimTerminalControl` | unary | Take the session's single-screen control lease, optionally by stealing it. |
| `WatchTerminalControl` | server stream | Lease changes, starting with an immediate snapshot. |

Every method is authenticated by `session_token`, resolved to an identity and then to the OS user the
serving host runs terminals as. An unknown or expired token is `UNAUTHENTICATED`; a known identity
with no OS-user mapping is `PERMISSION_DENIED`, because only an operator can fix the second and the
caller can only retry the first.

## Two servers, one answer

Two processes serve this coordinate:

- **`tddy-daemon`** registers it for the claude-cli and sandboxed sessions it owns, over Connect-HTTP,
  the LiveKit common room, and its local UDS socket.
- **`tddy-coder`**'s session participant registers it for the session it *is*, over that session's
  LiveKit room.

A client resolving a session over LiveKit reaches the coder; the same session over HTTP reaches the
daemon. Neither writes handlers of its own: both call `build_terminal_session_entry`, so the replay
model, the offset arithmetic and the acknowledgement framing are one implementation. The coder
answers seven of the nine and refuses `StreamSessionTerminalIO` and `WatchTerminalControl` with
`Unimplemented` — see
[`docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md`](../../../docs/dev/todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md)
for what a client sees as a result. `packages/tddy-coder/tests/two_server_parity_acceptance.rs` opens
one session through both servers and asserts the seven answer identically — history offsets, stream
mode, control claim.

## The four ports

The crate owns terminal *behaviour*; a host owns the terminals. Four traits carry the difference, so
a handler has no per-host branch in it.

| Port | The host's answer |
|---|---|
| `TerminalSessionStore` | Which live terminal a `(session_id, terminal_id)` names — after the host's own auth. An empty `terminal_id` resolves to the reserved `"main"`. |
| `TerminalSession` | One live terminal: its replay ring, its stdout broadcast, its exit watch, its acked-offset watch, resize and input. |
| `TerminalRoster` | Starting, stopping and listing the tools a session runs. |
| `TerminalControl` | The single-screen control lease — claim, steal, and the change broadcast. |

`TerminalSession` carries two defaulted predicates, and both are load-bearing:

- **`resizable()`** (default `true`). A terminal that answers `false` has no PTY master, so there is no
  SIGWINCH to issue and nothing for a post-resize drain to discard. The drain exists to throw away
  output produced *before* a resize took effect; against a terminal that cannot resize it would throw
  away live bytes no replay chunk covers, leaving the client a silent gap.
- **`requires_control()`** (default `true`). The lease exists so two browser screens cannot fight over
  one terminal. A caller that *is* the process owning the PTY holds no control token and has no way to
  claim one, so a host must neither demand one when the stream opens nor re-check one per chunk.

## The replay contract

A reconnecting client is shown **the current last frame first**. `StreamTerminalOutput` emits, in
order:

1. **The mode prologue** — the mouse-tracking private modes still in effect, re-issued as
   `ESC[?<mode>h`. A client's VT reports mouse events only after it has seen the DECSET the agent TUI
   emitted once at startup, and the capture ring trims from the front. The prologue is its own frame,
   independent of any replay branch.
2. **The anchor frame** — a tail chunk of the replay ring, `DEFAULT_INITIAL_FRAME_BYTES` (8 KiB) by
   default, tagged with its absolute `start_offset` / `end_offset` and an `at_oldest` flag. A
   compile-time assertion pins that default above zero and below a megabyte: zero would make every
   open replay nothing and repaint from a blank screen, and a frame past a transport's per-message
   limit is the regression per-frame chunking exists to prevent.
3. **The initial ACK**, then the live tail.

Older bytes are not pushed. A client scrolling up calls `GetTerminalHistory(from_offset,
until_offset, max_bytes)` and walks backwards chunk by chunk until one arrives with `at_oldest = true`.
`StreamTerminalOutputRequest` and the bidi open frame both carry a `StreamReplayMode` and a
`from_offset`, so a reconnect resumes at the byte it stopped at rather than from the tip.

Every frame a stream emits — prologue, replay, ACK and live tail — carries the session id and the
**resolved** terminal id, so a client can drop output belonging to another terminal rather than paint
it into the wrong one.

The ring itself, the mouse-mode sniffing and the escape-boundary trimming are
[`tddy-task`'s `TerminalCapture`](../../tddy-task/docs/terminal-capture.md).

## Input-offset acknowledgement

`SendTerminalInput` carries a cumulative byte `input_offset` (0 = unset). Once the bytes reach the
PTY the applied offset is recorded as a monotonic max, and the output stream interleaves an ACK frame
— `SessionTerminalOutput { data: [], acked_input_offset: N }` — with the data. A fresh subscriber
receives the current offset up front. The browser renders the gap between sent and acknowledged as an
[enqueued-input overlay](../../../docs/ft/web/enqueued-input-overlay.md) on slow links.

The accumulator and the ack watch live on the shared task channel rather than on a handle, because a
host may rebuild its handle per RPC — an ACK raised on the input path has to reach an output stream
that is already open.

## The control lease

At most one screen drives a session's terminals. `ClaimTerminalControl(session_id, screen_id, steal)`
grants a `control_token` when the lease is free or already the caller's, denies with the current
holder's `screen_id` when another screen holds it, and always grants when `steal` is set — which
revokes the previous token and notifies every `WatchTerminalControl` subscriber. `SendTerminalInput`
and `StreamSessionTerminalIO` refuse a stale `control_token` with `PERMISSION_DENIED`, for terminals
whose `requires_control()` is true.

The lease is a port rather than state this crate keeps: a session is served by exactly one host, and
that host's other surfaces (session deletion, the LiveKit bridge) read and clear the same lease.

## Transports

`build_terminal_session_entry` produces a `tddy-rpc` `ServiceEntry`, which is what LiveKit and stdio
register. gRPC and Connect-HTTP reach the same implementation through
`TerminalSessionServiceTonicAdapter`, generated by
[`tddy-codegen`](../../tddy-codegen/docs/tonic-adapter.md) — nine delegations including the
bidirectional one, none written by hand.

The daemon mounts that adapter on its local UDS socket, which is what `tddy-sandbox-app` — the binary
running inside every jail — dials to open its terminal stream. Both mounts are built from the same
ports over the same `Arc`s, so the socket and the Connect-HTTP entry address one set of PTYs and one
control lease.

## Sandboxed sessions are ordinary terminals

A jailed session's PTY output arrives over the stdio bridge in `tddy-daemon-sandbox` and is captured
in its own `TerminalCapture`. `tddy-daemon`'s `terminal_session_adapter` binds that state to
`TerminalSession` beside the claude-cli one, and `DaemonTerminalSessionStore` is a composite that
resolves a sandbox session first and falls back to the CLI manager. So a jailed terminal replays,
anchors, pages and acknowledges exactly like every other one, through this crate's bridge and nothing
else.

Three of that adapter's answers are synthesised, because a jail's stdio bridge does not supply them:
the acked-offset watch is dropped immediately, so the bridge emits no ACK frames; `resize` is a no-op
and `resizable()` is `false`; and `pty_done` is derived from the stdout broadcast closing.

## Login shells

`StartTerminalSession` spawns the user's login shell in the session's checkout.
`login_shell::login_shell_for` resolves it from the passwd database for the OS user the host runs as,
falling back to `DEFAULT_LOGIN_SHELL`. That last resort is a named constant rather than an inline
literal so a host without it — a minimal Nix closure, Alpine — fails the spawn with the assumption
named.

## Tests

| Suite | What it pins |
|---|---|
| `terminal_session_service_acceptance.rs` | all nine methods answer at the registered coordinate |
| `terminal_session_bidi_acceptance.rs` | `StreamSessionTerminalIO` carries a session end to end |
| `terminal_history_parity_acceptance.rs` | `GetTerminalHistory` frames and offsets |
| `packages/tddy-daemon/tests/sandbox_terminal_parity_acceptance.rs` | a real sandbox session's served frames against literal expectations, across both replay modes, the prologue, forward fill and drifted-offset clamping |
| `packages/tddy-coder/tests/two_server_parity_acceptance.rs` | the daemon and the coder answer one session identically |
