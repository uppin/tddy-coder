# `terminal_session.TerminalSessionService` (tddy-terminal-rpc)

The nine RPCs that carry a terminal, and the one implementation behind them. The proto is
`packages/tddy-terminal-rpc/proto/terminal_session.proto`; the service is `src/service.rs`, with
the host port types in `src/service/ports.rs`; the streaming logic every method shares is `src/bridge.rs`.

Product contract: [terminal-sessions.md](../../../docs/ft/daemon/terminal-sessions.md).

## The surface

| RPC | Shape | What it does |
|---|---|---|
| `StreamSessionTerminalIO` | bidi | Raw terminal I/O for transports that can send a streaming request body. The first client message authenticates, names the terminal and chooses the replay mode; later messages carry stdin bytes. The web terminal opens every stream here with `SNAPSHOT`. |
| `StreamTerminalOutput` | server stream | Attach to a terminal's output: a snapshot, or a mode prologue and anchored replay, then the live tail. |
| `SendTerminalInput` | unary | One write to a terminal's stdin, carrying a cumulative `input_offset`. |
| `GetTerminalHistory` | server stream | Bytes older than what a stream replayed, addressed by absolute offsets. |
| `StartTerminalSession` | unary | Spawn the tool `kind` names — the login shell, or a CLI harness — in the session's checkout and return its `terminal_id`. |
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
model, the snapshot, the offset arithmetic and the acknowledgement framing are one implementation.
The coder answers eight of the nine, `StreamSessionTerminalIO` among them, and refuses only
`WatchTerminalControl` with `Unimplemented` (backlog:
`2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine`, which records what a client sees as
a result). `packages/tddy-coder/tests/two_server_parity_acceptance.rs` opens one session through both
servers and asserts the eight answer identically — history offsets, stream mode, control claim, and a
bidi `SNAPSHOT` open (`both_wirings_open_one_terminal_with_the_same_snapshot_over_the_bidi_stream`).

## The four ports

The crate owns terminal *behaviour*; a host owns the terminals. Four traits carry the difference, so
a handler has no per-host branch in it.

| Port | The host's answer |
|---|---|
| `TerminalSessionStore` | Which live terminal a `(session_id, terminal_id)` names — after the host's own auth. An empty `terminal_id` resolves to the reserved `"main"`. |
| `TerminalSession` | One live terminal: its capture (screen model and replay ring), its stdout broadcast, its exit watch, its acked-offset watch, resize and input. |
| `TerminalRoster` | Starting (from a `TerminalLaunch`), stopping and listing the tools a session runs. |
| `TerminalControl` | The single-screen control lease — claim, steal, and the change broadcast. |

The port types — `TerminalSessionPorts`, `TerminalRoster`, `TerminalControl`, `TerminalDescriptor`,
`ControlClaim`, `ControlChange`, `TerminalLaunch` and `TerminalKind` — live in the module
`tddy_terminal_rpc::service::ports` (`src/service/ports.rs`). `service` re-exports them, and the crate
root re-exports them from there, so `tddy_terminal_rpc::service::TerminalRoster` and
`tddy_terminal_rpc::TerminalRoster` both name them; `src/service.rs` keeps the handlers and
`build_terminal_session_entry`.

`TerminalSession` also provides **`snapshot_and_subscribe() -> Result<(ScreenSnapshot,
broadcast::Receiver<Bytes>), Status>`**: it locks `capture()`, takes
[`TerminalCapture::snapshot`](../../tddy-task/docs/terminal-capture.md#screen-snapshots), subscribes
to `subscribe_stdout()` and unlocks. Every producer appends and broadcasts under that same lock, so the
receiver yields exactly the bytes from `snapshot.offset` on — the
[sequence point](../../tddy-task/docs/terminal-capture.md#the-sequence-point). A capture whose model has
not been sized (`snapshot()` is `None`) is refused with `FAILED_PRECONDITION`, so an adapter that
cannot know its PTY's size needs no override to refuse. Every host uses the provided default.

`TerminalSession` carries two defaulted predicates, and both are load-bearing:

- **`resizable()`** (default `true`). A terminal that answers `false` has no PTY master, so there is no
  SIGWINCH to issue and nothing for a post-resize drain to discard. The drain exists to throw away
  output produced *before* a resize took effect; against a terminal that cannot resize it would throw
  away live bytes no replay chunk covers, leaving the client a silent gap.
- **`requires_control()`** (default `true`). The lease exists so two browser screens cannot fight over
  one terminal. A caller that *is* the process owning the PTY holds no control token and has no way to
  claim one, so a host must neither demand one when the stream opens nor re-check one per chunk.

## Snapshot opens

`StreamReplayMode::STREAM_REPLAY_MODE_SNAPSHOT = 2`, on the bidi open message or a
`StreamTerminalOutputRequest`, shows a client **the screen as it is now**, then only what is written
after it. `bridge::open_snapshot` serves it for both RPCs:

1. **Refuse if unsized.** A terminal whose capture has no grid is refused with `FAILED_PRECONDITION`
   before anything is streamed.
2. **Resize only when driving.** When the open carries a grid (`initial_rows`/`initial_cols` both
   nonzero), the terminal is `resizable()`, the open may drive the session (an opener without the
   lease, on either RPC, arrives with its grid zeroed by `service.rs`) and that grid differs from the model's, the PTY **and** the model are resized first, so the
   snapshot is serialized at the client's width.
3. **The snapshot frame** — `snapshot = true`, `data` = the repaint (it begins with RIS),
   `start_offset = 0`, `end_offset = snapshot.offset`, `snapshot_rows` / `snapshot_cols` = the grid
   it was serialized at — taken with `snapshot_and_subscribe`.
4. **The initial ACK**, then the **live frames**, each stamped with its `[start_offset, end_offset)`:
   the first starts at the snapshot's `end_offset` and each at the previous frame's `end_offset`, so a
   client can verify there is no gap and no duplicate.

There is no mode prologue frame (the snapshot carries the modes), no raw replay and no
`trigger_redraw`: the application is never sent SIGWINCH. `end_offset` on the snapshot is the anchor
for `GetTerminalHistory`.

A jailed terminal answers `SNAPSHOT` once its runner has reported a size (see
[Sandboxed sessions](#sandboxed-sessions-are-ordinary-terminals)); before that, the provided
`snapshot_and_subscribe` refuses it with `FAILED_PRECONDITION`.

### Resync at any point

A `SNAPSHOT` stream can be given a **fresh snapshot mid-stream**, at a new sequence point, without
reconnecting. Two things trigger it, and both take the same path:

- **The client asks.** `SessionTerminalInput.request_snapshot = 11` is honoured on **any** message of
  `StreamSessionTerminalIO`, not only the first. The input forwarder (`spawn_input_forwarder`)
  applies the message's own `data` to the PTY first, then signals the output half through a
  `tokio::sync::Notify` it shares with the live bridge (`OutputHalf::resync`). Asking is a read: it
  needs no lease and is never refused. Requests made while the bridge is busy coalesce into one.
- **The subscriber lagged.** `RecvError::Lagged` on the live broadcast is answered with a snapshot
  instead of a hole.

`LiveOutput::resync` calls `snapshot_and_subscribe()` again, **replaces** the broadcast receiver
(whatever the old one had queued is covered by the snapshot and dropped with it), emits a snapshot
frame and restarts offset stamping at its `end_offset`. The deltas after it are contiguous from that
point, as after the opening snapshot. A snapshot that cannot be taken is sent to the client as an
error and ends the bridge (`send_resync`). The bridge task holds the session only as a `Weak`,
upgraded for the moment of a resync: holding it strongly would keep the output broadcast alive, and
a stream on a terminal that died (a jail's, say) would never see `Closed` and never end.

`TAIL` and `FROM_OFFSET` streams carry no offsets and no snapshot frames, so they keep their byte
semantics: a lagged subscriber skips the dropped bytes, and `request_snapshot` is ignored. The split
`StreamTerminalOutput` has no input half and so no request channel, though a lag on a `SNAPSHOT`
open there is resynced the same way. A refusal or ACK frame a lagged subscriber missed is not
replayed; the ACK watch reports the current applied offset on its next change.

## The replay contract (`TAIL` / `FROM_OFFSET`)

The legacy modes serve non-web clients. A `TAIL` open shows **the current last frame first**, as raw
bytes. `StreamTerminalOutput` emits, in order:

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

In these modes a live tail frame carries no offsets. A `TAIL` open resizes the PTY (when driving),
drains the pre-resize broadcast and calls `trigger_redraw`; a `FROM_OFFSET` open forward-fills from
`from_offset` to the tip and does neither.

Every frame a stream emits — snapshot, prologue, replay, ACK, refusal and live tail — carries the
session id and the **resolved** terminal id, so a client can drop output belonging to another terminal rather than paint
it into the wrong one.

The screen model, the ring, the mode sniffing and the escape-boundary trimming are
[`tddy-task`'s `TerminalCapture`](../../tddy-task/docs/terminal-capture.md).

## Input-offset acknowledgement

`SendTerminalInput` carries a cumulative byte `input_offset` (0 = unset). Once the bytes reach the
PTY the applied offset is recorded as a monotonic max, and the output stream interleaves an ACK frame
— `SessionTerminalOutput { data: [], acked_input_offset: N }` — with the data. A fresh subscriber
receives the current offset up front. The browser renders the gap between sent and acknowledged as an
[enqueued-input overlay](../../../docs/ft/web/enqueued-input-overlay.md) on slow links.

**Reading needs no lease; writing does.** `StreamSessionTerminalIO` opens for every screen — a
watcher without the lease (or with a stale token, e.g. a screen re-opening after another took control)
is served the snapshot and the live output, with its grid ignored. Only writes are gated, per chunk,
by `bridge::spawn_input_forwarder`.

On `StreamSessionTerminalIO` each stdin message carries the same cumulative `input_offset`. The bidi
stream has no per-message answer, so a chunk carrying data under a token that does not hold the
lease — the opening message's included, and a resize, which travels as an OSC in the data — is answered on the **output**
half by a **refusal frame**: `data` empty, `refused_input_offset = 11` set to that chunk's cumulative
offset, `input_refusal = 12` the reason ("terminal controlled by another screen"). The chunk never
reaches the PTY. The forwarder and the stream stay open and judge every later chunk on its own, so a
screen that re-claims control resumes typing without reconnecting; an ACK past
`refused_input_offset` means input is flowing again.

The accumulator and the ack watch live on the shared task channel rather than on a handle, because a
host may rebuild its handle per RPC — an ACK raised on the input path has to reach an output stream
that is already open.

## The control lease

At most one screen drives a session's terminals. `ClaimTerminalControl(session_id, screen_id, steal)`
grants a `control_token` when the lease is free or already the caller's, denies with the current
holder's `screen_id` when another screen holds it, and always grants when `steal` is set — which
revokes the previous token and notifies every `WatchTerminalControl` subscriber. `SendTerminalInput`
and the `StreamSessionTerminalIO` open refuse a stale `control_token` with `FAILED_PRECONDITION`
("terminal controlled by another screen"), for terminals whose `requires_control()` is true; a later
bidi chunk under a stale token is answered with a refusal frame instead (see above).

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
anchors, pages, acknowledges and snapshots exactly like every other one, through this crate's bridge
and nothing else.

Its capture is sized by the jail, not by this crate: `tddy-sandbox-runner` sends a
`SessionFrame.terminal_resized` (`SandboxTerminalResized { cols, rows }`) after `openpty` and after
every resize the PTY accepted, in the same poll-gated backlog as `terminal_output`. The daemon's
host relay opts into `HostRelayConfig::with_terminal_events`, which delivers output and size changes
as `HostTerminalEvent`s on one ordered channel, and `sandbox_session::forward_jail_terminal_events`
appends output (broadcasting under the capture lock) and calls `TerminalCapture::resize` at the same
point in the stream. A jail that has not reported yet has an unsized capture, so the provided
`snapshot_and_subscribe` refuses `SNAPSHOT` with `FAILED_PRECONDITION` (see
[Snapshot opens](#snapshot-opens)).

Three of that adapter's answers are synthesised, because a jail's stdio bridge does not supply them:
the acked-offset watch is dropped immediately, so the bridge emits no ACK frames; `resize` is a no-op
and `resizable()` is `false`; and `pty_done` is derived from the stdout broadcast closing.

## The PTY action runtime and per-user settings

Two modules here serve the daemon's PTY sessions rather than the RPC surface. Both are reached by
`tddy-session-lifecycle`'s `CliSessionManager` through that crate's facade
(`pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};`), so its `crate::pty_runtime::…`
paths resolve unchanged.

| Module | Holds |
|---|---|
| `pty_runtime` | `PtyRuntime` and `PtySpawnSpec`: spawns an interactive tool as a registered `tddy_task` task. It resolves a target `os_user` up front — uid, gid and home through `tddy_daemon_kernel::privilege_drop` (re-exported here), a front-loaded `setpriv` launcher when the user differs from the daemon's, and the child's `HOME`/`PATH` overrides — then hands the resolved argv and env to `tddy_pty::PtyRuntime`, which has no notion of an OS user. An unresolvable user fails the task and reports the reason, without leaking a PTY pair |
| `tddy_user_config` | a re-export of `tddy_daemon_kernel::user_paths::{load_tddy_user_config, spawn_path_extra_for_home, TddyUserHomeConfig}`: per-OS-user settings from `~/.tddy/config.yaml` |

They are why this crate depends on `tddy-daemon-kernel`.

**Unix only, as it stands.** `pty_runtime`'s unconditional `pub use` of `privilege_drop` names two
`#[cfg(unix)]` kernel items (`resolve_pty_os_user`, `ResolvedPtyUser`), so the crate does not build
on a non-unix target.

## `pty_relay`'s modules

`pty_relay.rs` holds `PtyRelayConfig`, `run_pty_relay` and the request builders. Its three paths each
have a module: `pty_relay/room.rs` (the room-backed start-and-connect and connect-only paths,
`run_livekit_session`), `pty_relay/daemon_http.rs` (the daemon's Connect-HTTP calls:
`open_session_room_over_http`, `join_terminal_room`, the stub-token exchange and `connectrpc_post`) and
`pty_relay/grpc_terminal.rs` (the gRPC terminal fallback). `open_session_room_over_http` and
`join_terminal_room` are re-exported from `pty_relay`.

## Opening the session's room for the LiveKit relay

`pty_relay`'s LiveKit path (`tddy-tools pty-relay --livekit-url`) waits for the session's
`daemon-{instance_id}` participant, so it first calls `open_session_room_over_http(daemon_url,
session_token, session_id)`: a Connect-HTTP POST of `OpenSessionRoomRequest`
to `session_room.SessionRoomService/OpenSessionRoom`, returning the decoded
`OpenSessionRoomResponse`. `ConnectSession` is not used for this: it opens no room for a caller that
did not arrive over LiveKit. A daemon whose LiveKit cannot be reached fails the call naming the room.
`tests/pty_relay_room_opening.rs` pins that only the room service's path is hit, with the request body.

## Joining the room through the client port

`run_pty_relay(config, connector)` takes an `Arc<dyn RoomConnector>` from
[`tddy-room-client`](../../tddy-room-client/docs/room-client.md) and names no transport; `tddy-tools`
hands it LiveKit's. `join_terminal_room(config, livekit_url, server_identity, connector)` joins the
configured room (`tddy-lobby` when none is named) as `client_identity`, minting from the API
credentials for one hour and waiting thirty seconds for the server participant. The terminal stream
opens through `RoomSession::open_duplex("terminal.TerminalService", "StreamTerminalIO")`.
`tests/pty_relay_room_join.rs` pins the join request against an in-memory connector.

## Starting a terminal: `TerminalLaunch`

`StartTerminalSessionRequest.kind` says what a started terminal runs. The handler parses it with
`TerminalKind::from_request` — `""` or `"bash"` (surrounding whitespace ignored) is
`TerminalKind::Bash`; anything else is `TerminalKind::Agent(kind)` — resolves the login shell, and
hands the roster `start(session_id, &TerminalLaunch { kind, login_shell })`. Whether an agent kind
is one the host runs is the roster's decision, because only the host knows its harness registry:

- **`tddy-daemon`** (`CliManagerTerminalRoster`, in `tddy-session-lifecycle`) runs `Bash` with
  `launch.login_shell`, and resolves `Agent(kind)` through `cli_harness(kind)` — `INVALID_ARGUMENT`
  "unknown terminal kind" when unregistered — then `DaemonConfig::cli_harness_binary` and the
  descriptor's argv for a default `CliLaunch` with a fresh conversation id, spawned by
  `CliSessionManager::start_agent_terminal`.
- **`tddy-coder`** (`CoderTerminalRoster`) runs `Bash` only and refuses `Agent(_)` with
  `INVALID_ARGUMENT` "this session's terminals run bash only".

A kind is a registered harness name, never an argv, so a client cannot choose the binary.

## Login shells

A `Bash` terminal runs the user's login shell in the session's checkout.
`login_shell::login_shell_for` resolves it from the passwd database for the OS user the host runs as,
falling back to `DEFAULT_LOGIN_SHELL`. That last resort is a named constant rather than an inline
literal so a host without it — a minimal Nix closure, Alpine — fails the spawn with the assumption
named.

## Tests

| Suite | What it pins |
|---|---|
| `terminal_session_service_acceptance.rs` | all nine methods answer at the registered coordinate; the request's `kind` reaches the roster, and an empty or `"bash"` kind arrives as `Bash` with the login shell |
| `terminal_session_bidi_acceptance.rs` | `StreamSessionTerminalIO` carries a session end to end; a screen without the lease (or whose lease was taken, re-opening after a blip) watches the terminal while its keystrokes are refused; a keystroke under a stale token is answered with a refusal frame and typing resumes on the same stream once control is reclaimed |
| `stream_snapshot_acceptance.rs` | a `SNAPSHOT` open renders the server's screen in a reference emulator (`vt100`, dev-dependency) then only contiguous deltas equal to `replay_from(snapshot.end_offset)`; no redraw signal and no prologue frame; a new grid resizes first and snapshots at it; `StreamTerminalOutput` honours `SNAPSHOT`; an unsized terminal is refused |
| `stream_resync_acceptance.rs` | a mid-stream `request_snapshot` on a message that also types applies the keystroke, then yields a snapshot that renders the server's screen, followed only by deltas contiguous from it; a subscriber that lags a capacity-1 broadcast is resynced with a snapshot, not a hole; a `TAIL` stream that lags is never sent a snapshot frame |
| `terminal_history_parity_acceptance.rs` | `GetTerminalHistory` frames and offsets |
| `src/pty_runtime.rs` (6 unit) | the spawn spec carrying its target OS user; and, through the re-export, `tddy_daemon_kernel::privilege_drop`'s helpers: the `HOME`/`PATH` overrides for a target user, when a privilege drop is and is not required, and the `setpriv` launcher wrapping |
| `src/tddy_user_config.rs` (2 unit) | the `PATH` extra read from `~/.tddy/config.yaml`, and a missing file read as none |
| `packages/tddy-session-lifecycle/tests/sandbox_terminal_parity_acceptance.rs` | a real sandbox session's served frames against literal expectations, across the `TAIL` and `FROM_OFFSET` replay modes, the prologue, forward fill and drifted-offset clamping |
| `packages/tddy-coder/tests/two_server_parity_acceptance.rs` | the daemon and the coder answer one session identically |
