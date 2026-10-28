# Multiple Tools per Session (Bash tool)

> **Updated: 2026-07-28** — Lazy replay & scroll-up history. `StreamTerminalOutput` now sends the
> **current last frame first**, tagged with absolute byte offsets (`start_offset`/`end_offset`/
> `at_oldest`); older history is fetched on demand via the new `GetTerminalHistory` RPC as the user
> scrolls up, fetched **forward** from offset `0` toward the anchor, one chunk at a time, until
> `at_end` terminates the fill. `SessionTerminalOutput` gains `start_offset`/`end_offset`/`at_oldest`
> fields; `GetTerminalHistory` uses `from_offset`/`until_offset` + `at_end`. The duplicated PTY-
> over-RPC bridge logic is unified into the shared `tddy-terminal-rpc` crate; the daemon and coder
> terminal handlers now delegate to it. See § Lazy replay & scroll-up history below.
>
> **Updated: 2026-06-29** — PTY tools (claude-cli, bash) are spawned via `PtyRuntime` into the shared
> `TaskRegistry` and appear in `tasks.TaskService.ListTasks`. `PtyRegistry` holds resize/control
> handles keyed by `task_id`. Terminal RPCs remain the external contract (compat layer).
>
> **Updated: 2026-06-26** — `StreamTerminalOutputRequest` gains `initial_cols`/`initial_rows`: daemon resizes PTY before replay, drains stale broadcast, triggers SIGWINCH — eliminates 220-col garbling on browser reconnect. `PtyHandle::send_input` strips OSC resize escapes. `kill_all()` added for clean daemon shutdown. Capture limit raised 64 KB → 512 KB.
>
> **Updated: 2026-06-25** — Reframed from "attachable terminal sessions" to **running multiple
> tools per session**. A session hosts identified tool instances; this release defines the **Bash**
> tool (a shell that takes no inputs and opens on the worktree). The RPC surface keeps its
> terminal-oriented names; each started terminal is a Bash tool instance.

## Summary

A tddy session can run **multiple tools**, not just the single `claude` CLI process it spawns
today. Each tool is a `portable-pty` process managed by `tddy-daemon`'s `ClaudeCliSessionManager`,
reusing the existing `PtyHandle` mechanic (PTY master + broadcast output + rolling capture-replay +
stdin channel + exit-monitor cleanup).

The first additional tool is **Bash** — a shell tool defined the same way the Claude CLI tool is,
but it takes **no inputs** and simply opens on the session's worktree directory.

A started terminal can instead run any registered CLI harness — an **agent terminal**: Warp, Claude
Code, Cursor Agent, OpenCode or Muse Code beside the session's main agent, in the same worktree. See
[Agent terminals](#agent-terminals).

Every tool carries a stable **terminal id**: the original `claude` tool is identified by the
reserved id `"main"` (kind `"claude-cli"`), and each started Bash tool gets a fresh id (kind
`"bash"`). Clients manage tools over RPC — start, stop, list — and address a specific tool's I/O by
id. This release is **RPC-only** (no web UI integration).

## User Story

As a client of `tddy-daemon`, I want to run extra tools (starting with Bash) inside a running
session and manage them over RPC, so I can run a shell in the session's worktree alongside the main
`claude` tool — with each tool individually identified and addressable.

## Acceptance Criteria

### Identification
1. Every tool is identified. The main `claude` tool is listed under the reserved id `"main"`;
   started Bash tools receive fresh, unique ids distinct from `"main"`.
2. `ListTerminalSessions(session_id)` returns one `TerminalSessionInfo{terminal_id, kind, pid}`
   per running tool of that session — `kind` is the main tool's session type (`"claude-cli"`, …) for
   `"main"`, **`"bash"`** for started Bash tools, and the harness's session type (`"warp-cli"`,
   `"muse-cli"`, …) for a started agent terminal.

### Start / Stop
3. `StartTerminalSession(session_id, kind)` starts the tool `kind` names, in the session's worktree,
   and returns the new `terminal_id`. An empty `kind` (or `"bash"`) is the **Bash tool**: the OS
   user's login shell, with no inputs; any other `kind` is a registered CLI harness, started as an
   [agent terminal](#agent-terminals). The shell is the
   user's passwd `pw_shell`, else the serving process's `$SHELL`, else `/bin/bash` — the passwd entry
   first because a daemon started by systemd or nix has a `$SHELL` of its own that is not the target
   user's interactive shell, and a terminal opened with it comes up in the wrong shell with none of
   the user's rc files. A `pw_shell` that refuses logins (`/usr/sbin/nologin`, `/bin/false`) is
   skipped.
4. The Bash tool uses the same PTY mechanic as the main tool: stdin is writable, output is
   broadcast and captured for replay.
5. `StopTerminalSession(session_id, terminal_id)` terminates the tool's process and removes it from
   the registry; it no longer appears in `ListTerminalSessions`.
6. `StopTerminalSession` with `terminal_id = "main"` is rejected with `INVALID_ARGUMENT` — the main
   `claude` tool is managed through session lifecycle (`SignalSession` / `DeleteSession`), not this
   API.

### I/O addressed by terminal id
7. The terminal I/O RPCs (`StreamSessionTerminalIO`, `StreamTerminalOutput`, `SendTerminalInput`)
   accept an optional `terminal_id`. Empty `terminal_id` resolves to `"main"` (backward compatible
   with existing clients).
8. Output streaming and input delivery target the addressed tool; an unknown `terminal_id` yields
   `NOT_FOUND`.

### Auth
9. Every RPC validates `session_token` (→ GitHub user → OS user) exactly like the existing session
   RPCs; an invalid token yields `UNAUTHENTICATED`.

## Agent terminals

`StartTerminalSessionRequest.kind` (field 4) chooses what a started terminal runs. Empty or `"bash"`
is the login shell; otherwise it is the session type of a registered CLI harness — `claude-cli`,
`cursor-cli`, `opencode-cli`, `muse-cli`, `warp-cli`. A kind names a registered harness, never a
command line, so a client cannot make the daemon run an arbitrary binary through a terminal.

An agent kind spawns that harness's **fresh interactive start**, as its CLI harness descriptor builds
it for a default launch, in the session's worktree under a fresh UUIDv7 `terminal_id`:

- no model, no first prompt, no resume, no provider route;
- a fresh conversation id for a harness that takes one (Claude Code's `--session-id`), never the
  session's own;
- no chat id for Cursor, which starts a new chat.

The binary is resolved exactly as for that harness's own sessions: its env var, then
`cli_harnesses.<type>.binary_path` in `daemon.yaml`, then the default name on `PATH`.
`ListTerminalSessions` lists the tab with the harness's session type as its `kind`, and
`StopTerminalSession` kills it and removes it from the list.

Refusals:

| Request | Status |
|---|---|
| `kind` is neither empty, `bash`, nor a registered harness | `INVALID_ARGUMENT` "unknown terminal kind …"; nothing is spawned |
| an agent kind on a **room-backed** (tddy-coder) session | `INVALID_ARGUMENT` "this session's terminals run bash only" |
| any started terminal on a **sandboxed** session | `FAILED_PRECONDITION` |
| the harness binary cannot be spawned | the spawn error, as for a Bash tool |

Like Bash tools, agent terminals get **no session environment** — no git identity, no `TDDY_SOCKET`,
no provider route, no `os_user` — are not stopped by `DeleteSession` or the main tool's exit, and are
**not persisted**: they do not resume after a daemon restart or a session resume. The web surface is
the `+` menu of [Session Terminal Tabs](../web/session-terminal-tabs.md).

Known limitation: an agent terminal gets no hooks or activity status of its own. A `claude-cli` tab
inside a `claude-cli` session shares the worktree's `.claude/settings.local.json` hooks, so its
activity may be reported against the main session — see
[`docs/dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md`](../../dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md).

## Terminal Control Ownership (Single-Screen Mutex)

> **Updated: 2026-06-26** — Extends the multi-tool RPC surface with a per-session control mutex
> so that exactly one screen can send input at a time.

### Summary

A tddy session's terminals — the main `claude` terminal plus any Bash tools — may be connected
from multiple browser tabs or clients simultaneously. Today each client can send input, causing
conflicting keystrokes. The control mutex ensures exactly **one screen is the controller** at any
given moment; all other connected screens are **observers** (they still receive output).

### Acceptance Criteria

#### Control Lease

10. `ClaimTerminalControl(session_id, screen_id, steal=false)` grants a `control_token` when the
    session is uncontrolled or already controlled by the same `screen_id`.
11. `ClaimTerminalControl(session_id, screen_id, steal=false)` is **denied** (returns
    `granted=false`, `current_holder_screen_id` set) when another `screen_id` holds the lease.
12. `ClaimTerminalControl(session_id, screen_id, steal=true)` always grants a new `control_token`,
    evicting the previous holder and emitting a `ControlChangeEvent` to all
    `WatchTerminalControl` subscribers for that session.
13. A session with no active lease is "uncontrolled": all input RPCs are accepted regardless of
    `control_token` (backwards-compatible with clients that do not yet send the field).

#### Input Enforcement

14. `SendTerminalInput` with a `control_token` that does not match the current lease returns
    `FAILED_PRECONDITION` ("terminal controlled by another screen"). `StreamSessionTerminalIO` opens
    without the lease (watching is free) and answers each such input chunk with a refusal frame
    instead, keeping the stream open.
15. The enforcement is transport-agnostic: the token is validated at the `ClaudeCliSessionManager`
    level, independent of whether the RPC arrived over HTTP Connect or LiveKit.

#### Watch / Notification

16. `WatchTerminalControl(session_id, control_token)` emits an immediate snapshot event
    (`TerminalControlEvent`) containing `holder_screen_id` and `you_are_controller` (true iff
    the subscriber's token matches the current lease).
17. When the lease changes (steal), all active `WatchTerminalControl` subscribers receive a new
    `TerminalControlEvent` reflecting the new holder.

#### Auth

18. `ClaimTerminalControl` and `WatchTerminalControl` require a valid `session_token`
    (→ GitHub user → OS user); an invalid token yields `UNAUTHENTICATED`.

### Non-goals (this iteration)

- No heartbeat-based auto-release when the controlling browser tab closes. A disconnected
  controller retains the lease; the next screen reclaims via `ClaimTerminalControl(steal=true)`.
- No persistence across daemon restart (the control registry is in-memory).
- The lease granularity is **per-session**, not per-terminal-id within a session.

#### Daemon-side holders

A screen need not be a browser. The project-home agent's `ClaimSessionTerminal` claims the lease
with `steal=true` as `project-home:<home session id>`, and the daemon keeps the token on its behalf;
`SendSessionKeys` verifies it before every write, also for a jailed terminal, which does not check the
lease on its own input path. A browser takes control back by its own steal. See
[Project concept § Typing into a terminal](project-concept.md#typing-into-a-terminal).

---

## Session-scoped RPC routing & daemon-direct lifecycle

> The daemon is the **bootstrap/directory authority** and the **direct target for lifecycle
> control**; session-scoped `ConnectionService` methods for a LiveKit-backed (tddy-coder)
> session are served by the coder's own LiveKit participant. See
> [Session Participant RPC & Metadata](../coder/session-participant-rpc.md) for the coder side.

### Bootstrap / directory boundary

The daemon participant (`daemon-{instanceId}`) serves the calls the web makes **before or
without** an attached session participant:

- `StartSession`, `ConnectSession`, `ResumeSession`
- `ListSessions`, `ListProjects`, `ListAgents`, `ListTools`, `ListEligibleDaemons`,
  `ListProjectBranches`

### Session-scoped surface delegated

For a LiveKit-backed session the coder's participant (`daemon-{instanceId}-{sessionId}`) serves
`ListExecTools`, `ListSessionToolCalls`, `ExecuteTool`, VNC and screen-sharing on
`connection.ConnectionService`, and the terminal family on
`terminal_session.TerminalSessionService`. The daemon serves all of it for **non-LiveKit**
(claude-cli / cursor-cli / opencode-cli / muse-cli / warp-cli / workspace) sessions, where no coder participant exists.

The coder answers **eight** of the nine terminal methods, `StreamSessionTerminalIO` among them, and
refuses only `WatchTerminalControl` with `Unimplemented` — its control lease is a permanent
self-grant, so a watch event from it would tell every screen it is the controller. Nothing in the web
reaches that method at this participant. For the eight both serve — including a bidi `SNAPSHOT` open,
which shows a coder-served terminal the same screen the daemon would — one session answers
identically whichever server a client reaches, asserted at the wire by
`packages/tddy-coder/tests/two_server_parity_acceptance.rs`.

### `DeleteSession` / `SignalSession` — daemon-direct

The web calls `DeleteSession` / `SignalSession` **directly** on the daemon participant
(`daemon-{instanceId}`) with the caller's `session_token`; the coder is **not** on the path.
The daemon validates the token (GitHub user → OS user → session ownership) exactly as it does
for every other session RPC, performs process teardown / signalling, updates `.session.yaml`,
and returns the result. Daemon errors surface **verbatim** to the web caller. Serving these
daemon-direct keeps lifecycle control available even when the coder participant is
unresponsive, and lets the sessions list delete/signal an unattached row without a relay hop.

### Inspector data for sessions with no LiveKit participant

`SessionEntry` carries `bytes_in` / `bytes_out` / `last_data_received_at` fields. The daemon
populates these in `ListSessions` from the `GrpcSessionTerminal` traffic meter for
claude-cli / cursor-cli / opencode-cli / muse-cli / warp-cli / workspace sessions it owns (live counters), and reports zero / empty
for tddy-coder sessions that have no LiveKit participant (stopped). The web inspector renders
these when no per-session live runtime exists (see
[Session Drawer Screen § Inspector I/O bytes + last-data-received](../web/session-drawer.md#inspector-io-bytes--last-data-received)).

### What stays the same

- Process lifecycle ownership stays with the daemon: it spawns and tears down the coder
  process and owns the `.session.yaml` record.
- `session_list_enrichment.rs` (`SessionListStatusDisplay`) keeps enriching `SessionEntry`
  from disk for inactive and directory-listed sessions.
- Auth model is unchanged: `session_token` → GitHub user → OS user → session ownership,
  validated at the daemon for every `DeleteSession` / `SignalSession` (always daemon-direct).
- claude-cli / cursor-cli / opencode-cli / muse-cli / warp-cli / workspace sessions are served by the daemon on every coordinate.

---

## Lazy replay & scroll-up history

> **Added: 2026-07-28** — Replaces the eager full-capture replay with a last-frame-first model so a
> reconnecting/refreshing client sees content immediately, with older history loaded lazily as the
> user scrolls up. A reconnecting terminal resumes by offset instead of re-replaying the whole
> retained buffer, and a transient transport blip no longer evicts the runtime.

### Summary

Historically `StreamTerminalOutput` replayed the **entire** retained capture buffer on connect,
then tailed live output. For large scrollbacks this delayed the first paint and re-sent stale
pre-resize bytes. The replay model is now **lazy**: the server sends the **current last frame**
first (the bytes a full-screen client needs to render immediately), tagged with absolute byte
offsets, then tails live output. Older history is fetched on demand as the user scrolls up via
the new `GetTerminalHistory` RPC — fetched **forward** from offset `0` toward the anchor, one
chunk at a time, until the capture ring's anchor is reached.

### Wire contract

- `SessionTerminalOutput` carries three offset fields. In a `TAIL` / `FROM_OFFSET` stream they are
  populated **only on the initial replay frame** (live tail frames carry `0`); in a `SNAPSHOT` stream
  **every** data frame carries them (see [Snapshot on (re)connect](#snapshot-on-reconnect)):
  - `start_offset` — absolute offset of the first byte in `data`.
  - `end_offset` — absolute offset after the last byte in `data` (the **anchor** for older-history fetches).
  - `at_oldest` — `true` when the chunk reaches the capture ring's oldest retained byte (no older
    history exists to load by scrolling up).
- `GetTerminalHistory(session_token, session_id, terminal_id, from_offset, until_offset, max_bytes)` —
  server-streaming; yields **one** `TerminalHistoryChunk{data, start_offset, end_offset, at_oldest, at_end}`
  starting at `from_offset`, bounded by `until_offset` (the anchor), then closes. The client appends
  the chunk, advances `from_offset` to the chunk's `end_offset`, and calls again until `at_end = true`
  (reached `until_offset` / the capture tip).

### Snapshot on (re)connect

A terminal that connects or reconnects is shown **the current screen**, rendered correctly and at
once — never a stale or blank screen waiting for a resize. The host (daemon or coder) keeps a **screen
model** for every terminal, fed by its output at the PTY's size, and `StreamReplayMode::SNAPSHOT` on
the bidi `StreamSessionTerminalIO` open message (or on `StreamTerminalOutputRequest`) asks for it.

- **The snapshot frame** comes first: `snapshot = true`, one repaint that reproduces the visible
  screen and what is needed to render and drive it — both screen buffers with the alternate screen
  active or not, cursor position and visibility, colours and attributes, scroll region,
  origin/autowrap modes, saved cursor, charsets, tab stops, and the input modes (application
  cursor/keypad, bracketed paste, mouse tracking and encoding, focus reporting, cursor style). It
  begins with RIS, so a client writes it into its terminal as-is. `snapshot_rows` / `snapshot_cols`
  give the grid it was serialized at — the PTY's.
- **Ordering guarantee.** The snapshot reflects every output byte below its `end_offset` and none at
  or above it. Every later frame carries `start_offset` / `end_offset`; the first starts at the
  snapshot's `end_offset` and each starts where the previous ended — no gap, no duplicate. The
  snapshot's `end_offset` is the anchor for scroll-up history.
- **No nudging.** A snapshot open sends no SIGWINCH, no mode prologue and no raw replay. The PTY is
  resized to the client's grid first only when the open may drive the session and that grid differs.
- **Input on the same stream.** Stdin messages carry a cumulative `input_offset` and are acknowledged
  with ACK frames as on `SendTerminalInput`.
- **Watching needs no lease; typing does.** Any screen may open the stream and is shown the screen
  and its live output — including one without the lease, or one re-opening after another screen took
  control. Its grid is ignored (it cannot resize the PTY). A chunk presented without the control
  lease, the opening message's included, is dropped —
  it never reaches the PTY — and answered with a refusal frame (`refused_input_offset`,
  `input_refusal` "terminal controlled by another screen"); the stream stays open, so re-claiming
  control resumes typing.
- **Jailed terminals snapshot at the jail's size.** The daemon cannot see or resize a sandboxed PTY,
  so the runner inside the jail reports the size it applied — once when the PTY opens and again
  after every resize — in order with its output. The daemon's screen model follows each report at
  that point in the output stream, so the snapshot is serialized at the jail's current grid. A jailed
  terminal that has not reported a size yet refuses a `SNAPSHOT` open with `FAILED_PRECONDITION`; it
  is never serialized at a guessed size.
- A watcher whose grid differs from the PTY's sees the snapshot at the PTY's size.

The web terminal opens **every** stream this way (see [Web terminal](../web/web-terminal.md)).

### Resync at any point

> **Added: 2026-10-25** (`#term-snapshot` 2/4, [#761](https://github.com/uppin/tddy-coder/pull/761))

A long-lived stream can still diverge — a slow link, a subscriber that cannot keep up. On a stream
opened with `SNAPSHOT` the client can be shown the current screen again **at any time**, without
reconnecting:

- **On request.** `SessionTerminalInput.request_snapshot`, on **any** message of
  `StreamSessionTerminalIO` (not only the first). Input bytes on the same message are applied to the
  PTY first; then the host sends a snapshot frame at a new sequence point and continues live from its
  `end_offset`, with the same ordering guarantee as the opening snapshot. Asking is a read: a watcher
  without the control lease may ask, and the request is never refused.
- **On lag.** A subscriber the host's live output outran is sent a fresh snapshot instead of a gap.
- **In the web terminal.** The browser checks every frame's `start_offset` against the previous
  `end_offset`. A frame that does not continue the screen is not painted, and the terminal sends one
  `request_snapshot` on the same stream; it keeps the last consistent screen until the snapshot lands.

`TAIL` / `FROM_OFFSET` streams are unchanged: they carry no snapshot frames, ignore
`request_snapshot`, and a lagged subscriber still skips the dropped bytes. No resync is timed or
heuristic — only a request, a lag or a detected gap triggers one.

### Legacy replay modes (`TAIL` / `FROM_OFFSET`)

`StreamTerminalOutput`, `SendTerminalInput` and the `TAIL` / `FROM_OFFSET` modes remain for non-web
clients. `StreamTerminalOutputRequest` (and the bidi open message) carry a `mode` + `from_offset` so a
terminal that has already synced its state resumes by offset instead of re-replaying the whole
retained buffer.

- **`TAIL`** (default, first connect): the server sends the mode prologue + the current last-frame
  tail chunk (tagged with absolute offsets), resizes the PTY to the client's dimensions, drains the
  pre-resize broadcast, then bridges live output. The resize is **driving**, not watching: it is
  applied only when the open's `control_token` holds the session's control lease, or nobody holds
  it. A screen watching a session another screen drives opens at the driver's size, and resizes it
  only once its own claim lands (the client re-sends its grid each time it gains the lease).
- **`FROM_OFFSET`** (reconnect): the server sends the mode prologue + chunked catch-up via
  `replay_from(from_offset, tip, …)` until `at_end`, then live output. No tail chunk, no PTY
  resize/drain — a terminal that already holds state up to `from_offset` receives only the bytes it
  missed, with no duplicate content.

The replay params are read only from the first message of a bidi stream. The legacy prologue carries
the mouse modes only, so a `TAIL` / `FROM_OFFSET` attach is not told the program is on the alternate
screen; a snapshot is.

### Room-carried terminals (`terminal.TerminalService/StreamTerminalIO`)

> **Added: 2026-10-25** (`#term-snapshot` 4/4, [#763](https://github.com/uppin/tddy-coder/pull/763))

A session carried over its own LiveKit room serves its terminal on `terminal.TerminalService`
instead of `StreamSessionTerminalIO`, and it opens the same way: with the current screen, not a
replay of raw output and a forced redraw.

- **Snapshot on open.** The first frame of every `StreamTerminalIO` is a snapshot
  (`TerminalOutput.snapshot = true`): one repaint of the visible screen beginning with RIS, taken at
  a sequence point. Every frame carries `start_offset` / `end_offset` with the same ordering
  guarantee as the host's snapshot: the first delta starts at the snapshot's `end_offset`, each
  later one where the previous ended.
- **Resync at any point.** `TerminalInput.request_snapshot` on any message after the first (the
  opening snapshot already answers the first) is answered with a fresh snapshot at a new sequence
  point; bytes on the same message are applied first. A subscriber the PTY's output outran is sent
  one unasked.
- **No nudging.** No raw replay, no SIGWINCH. A PTY terminal whose size is not yet known refuses the
  open with `FAILED_PRECONDITION`, never serializing at a guessed grid.
- **The coder's own TUI answers it too.** `TerminalServiceVirtualTui` stamps each connection's
  output with offsets and sends its first frame, and every full repaint (a `request_snapshot`, a
  resize), led by RIS and flagged as a snapshot. Answering a request repaints at the current size —
  it never resizes — and mouse tracking is re-enabled after each repaint.
- **In the web terminal.** The room feed asks for a snapshot in the first message of every open, and
  checks every frame's offsets; a frame that does not continue the screen is dropped and one
  `request_snapshot` goes up the same stream.

A client that reads only `data` still renders a snapshot correctly, because it begins with a
terminal reset. Scrollback for room-carried sessions still comes from the host's
`GetTerminalHistory`; the room's offsets are the host capture's, so the fill is anchored.

### Client reconnect & null-client pause

The web client (`GrpcSessionTerminal`) opens with `SNAPSHOT` on every (re)connect. Its `client` is
`ConnectionClient | null`: a **null client** (a transient transport blip) **pauses** the terminal — it
stays mounted (its scrollback and the ghostty instance survive), input is queued — and re-opens with a snapshot when a
non-null client returns. Only a stream-end with a **valid** client (a real `pty_done`) evicts the
runtime.

### Client scroll-up protocol

1. On connect, the client receives the snapshot frame (or, on a legacy open, the initial replay
   frame) carrying `end_offset` (the anchor) and `at_oldest`.
2. When the user activates "Load earlier output" (or scrolls up on the live terminal) and `at_oldest`
   is false, the client shows a loading indicator, forward-fills a second, read-only older-history
   "page" terminal **in the background**, and calls
   `GetTerminalHistory(from_offset = 0, until_offset = end_offset)`, appending the returned chunk's
   bytes to the page terminal.
3. The next call uses `from_offset = <previous chunk's end_offset>`, advancing forward toward the
   anchor.
4. A chunk with `at_end = true` (or an empty/null chunk) terminates the forward fill — the client
   stops issuing further calls, the loading indicator disappears, and the page terminal **swaps to
   the foreground** (the live terminal stays mounted underneath and current). "Back to live" (or a
   scroll-down-at-bottom gesture on the page terminal) swaps back instantly.

The web client's forward-fill state machine lives in
`packages/tddy-web/src/lib/terminalHistoryLoader.ts` (`TerminalHistoryForwardLoader`);
`GhosttyTerminalSession` owns the overlay double-buffer paging end-to-end: it captures the anchor from
the initial frame, renders two interchangeable overlaid terminals (live `scrollback: 0` always
mounted & streaming; page `scrollback > 0`), shows a loading indicator while forward-filling the
page terminal in the background, swaps it to the foreground on `at_end`, and swaps back on
"Back to live" / scroll-down-at-bottom — all via the history fetcher its
`TerminalFeed` carries (`GrpcSessionTerminal` builds it for a host-served session). (The live terminal stays at `scrollback: 0` to preserve the
no-duplicate-pane fix; the page terminal carries `scrollback > 0`. The
`onRegisterLoadOlderHistory` indirection is removed.)

### One PTY-over-RPC surface

`terminal_session.TerminalSessionService` is the terminal surface, and `tddy-terminal-rpc` owns all
of it: the proto, the nine handlers, and the `bridge` module carrying the
streaming, snapshot, replay, ACK, refusal, resize and input-forwarding logic behind the `TerminalSession` /
`TerminalSessionStore` async traits. There is exactly one terminal message set and one implementation
of the offset contract.

Two processes serve the coordinate. `tddy-daemon`'s `DaemonTerminalSessionStore` resolves a
claude-cli `PtyHandle` or a jailed session's PTY — a composite, so a sandboxed terminal is an ordinary
terminal to the bridge rather than a branch inside it. `tddy-coder`'s `CoderTerminalSessionStore`
resolves its own bash tabs. Neither writes a handler. The screen model and snapshot live in
[`tddy-task`'s `TerminalCapture`](../../../packages/tddy-task/docs/terminal-capture.md).

Implementation reference:
[terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md).

The **client** half is in the same crate: `pty_relay::{PtyRelayConfig, run_pty_relay}` is the relay
that attaches a local terminal to a session — spawning a command in a local PTY, or starting and
connecting to a daemon session (including a sandboxed one) over HTTP or LiveKit, four dispatch modes
selected by which fields are set. `tddy-tools pty-relay` keeps only the clap surface that builds the
config. Both relays share one termios guard and terminal-size probe (`local_terminal`) rather than
declaring one each.

Attaching to a daemon session over **HTTP** never touches LiveKit, so a LiveKit server that is down
or misconfigured does not affect it. Over **LiveKit** (`--livekit-url`) the relay first asks the
daemon to open the session's room with `OpenSessionRoom`, then joins it and waits for the daemon's
participant; a daemon whose LiveKit cannot be reached fails that call, naming the room, and the
relay exits with it rather than waiting for a participant nobody put there. See
[Session rooms](session-room.md#when-the-room-is-created).

---

## Non-goals (out of scope)

- Persisting tools across daemon restart (tools are in-memory, like the main terminal today).

> **Update (session-terminal-tabs):** two former non-goals are now delivered — **Web UI integration**
> (a terminal tab bar; see [Session Terminal Tabs](../web/session-terminal-tabs.md)) and **a LiveKit
> bridge for Bash tools** (coder/LiveKit sessions serve `terminal_id`-addressed terminal RPCs from
> their own participant; see [Session Participant RPC & Metadata](../coder/session-participant-rpc.md)).
> The PTY plumbing that both the daemon and the coder use now lives in the shared `tddy-pty` crate.
- Tools that take inputs. A `kind` is a tool choice, not tool inputs: Bash and agent terminals both
  start with no per-start inputs.
- Configurable Bash binary — the Bash tool is built-in (the OS user's login shell, falling back to
  `tddy_terminal_rpc::login_shell::DEFAULT_LOGIN_SHELL`); no config entry is required.
  *(Added: 2026-06-25.)*

## Architecture

### Registry & identity
`ClaudeCliSessionManager`'s registry becomes two-level: `session_id → (terminal_id → PtyHandle)`.
`PtyHandle` carries `terminal_id` and `kind` (the main tool's session type, `"bash"`, or an agent
terminal's harness session type). The main `claude`
tool is inserted under the reserved id `MAIN_TERMINAL_ID = "main"`; `get(session_id)` remains a
convenience that returns the `"main"` tool so existing call sites are unchanged. The PTY
exit-monitor removes the tool by `(session_id, terminal_id)`.

### Tools & spawn
`spawn_in_pty` is generalized to accept a prebuilt `argv` plus `terminal_id`/`kind`, so both the
`claude` tool (`build_claude_argv`) and the Bash tool (`[shell_path]`) share the same I/O
threading, capture, and cleanup. The Bash tool's shell is resolved by
`tddy_terminal_rpc::login_shell::login_shell_for` at the RPC layer; the manager takes a resolved
`shell_path` argument (no test-only branches) and labels the instance kind `"bash"`. An agent
terminal's argv is built by the daemon's terminal roster from the harness descriptor and spawned by
`CliSessionManager::start_agent_terminal` under the harness's session type as its kind. Neither takes
per-start inputs.

### Lifecycle
`StopTerminalSession` signals the tool's pid (reusing the SIGTERM/SIGKILL helper used by session
deletion) and removes the registry entry; this is idempotent with the exit-monitor's own removal.

### RPC surface (`terminal_session.TerminalSessionService`)
All nine terminal methods answer at this coordinate: the three lifecycle methods
`StartTerminalSession` / `StopTerminalSession` / `ListTerminalSessions` (terminal-named for
continuity; each operates on a tool instance), the four I/O methods, and the two control methods.
Every I/O message carries an optional `terminal_id`, empty resolving to `"main"`.

A client reaches it at the same endpoints as every other service the daemon registers — `/rpc` over
Connect-HTTP, the LiveKit common room, and the local UDS socket — because it is a `ServiceEntry`
registered beside them rather than a second endpoint. The generated tonic adapter is what the socket
mounts, which is how `tddy-sandbox-app` inside a jail opens its bidirectional terminal stream.
