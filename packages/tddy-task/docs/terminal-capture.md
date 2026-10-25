# Terminal capture (screen model and mode-aware replay ring)

`tddy_task::TerminalCapture` (`src/terminal_capture.rs`) is the replay buffer behind every PTY-backed
task. It is what a late subscriber — a browser opening a terminal stream, a reconnecting LiveKit
client — is shown so the terminal is not blank until the application next repaints.

It holds three things, all fed by `append`:

- a **screen model** — an [`avt`](https://crates.io/crates/avt) `Vt` sized to the PTY — from which a
  [`ScreenSnapshot`](#screen-snapshots) of the visible state is serialized;
- a **byte ring** of the most recent raw output, which the legacy replays and `GetTerminalHistory`
  read;
- **sticky input-mode state** sniffed from the stream, which survives ring eviction.

## Why the modes are tracked separately

A terminal only reports clicks, drags and scrolls once **its own VT instance** has observed a
mouse-tracking DECSET in its byte stream. An agent TUI emits those bytes **once, at startup**.

The ring retains the last `CAPTURE_LIMIT_BYTES` (64 KiB) and trims from the front, so after ~64 KiB
of output the startup DECSETs are gone. Nothing re-emits them: `PtyHandle::trigger_redraw()` sends
SIGWINCH, and SIGWINCH makes an application redraw its *content* — it does not re-issue private
modes. A client attaching after that point could therefore never learn the terminal was in
mouse-tracking mode, and silently dropped every mouse event while keyboard input kept working.

So the capture sniffs mode changes as bytes pass through and remembers the input modes currently
in effect. That state costs no ring space, so it survives eviction indefinitely. The sniffer tracks:

| Sequence | State |
|---|---|
| DECSET/DECRST `1000` `1002` `1003` | mouse tracking (normal, button-event, any-event) |
| DECSET/DECRST `1005` `1006` `1015` `1016` | mouse encoding (UTF-8, SGR, urxvt, SGR-pixel) |
| DECSET/DECRST `1004` | focus reporting |
| DECSET/DECRST `2004` | bracketed paste |
| DECSET/DECRST `47` `1047` `1049` | alternate screen |
| DECKPAM / DECKPNM (`ESC =` / `ESC >`) | application keypad |
| DECSCUSR (`ESC[<n> q`) | cursor style (`0`, the default, is not recorded) |
| RIS (`ESC c`) | clears all of it |

A [snapshot](#screen-snapshots) restores every input mode in that table; the legacy
[`mode_prologue`](#api) re-issues the mouse family only.

## Screen snapshots

`snapshot() -> Option<ScreenSnapshot>` is the terminal's visible state as one repaint:

```rust
pub struct ScreenSnapshot {
    pub data: Vec<u8>, // the repaint
    pub offset: u64,   // the sequence point: every byte below it is reflected, none at or above it
    pub rows: u16,     // the grid `data` was serialized at
    pub cols: u16,
}
```

`data` is RIS, then the screen model's dump, then the restore sequences for the input modes in
effect (application keypad, DECSCUSR cursor style, and each tracked DECSET). Written into a fresh VT
of `rows`×`cols` it reproduces both screen buffers with the alternate one active or not, cursor
position and visibility, the SGR pen, scroll region, origin/autowrap/insert modes, saved cursor,
charsets and tab stops, and the input modes. Because it begins with RIS, a byte-only client writes it
into a live terminal as-is. `offset` is `end_offset()` at the moment it was taken.

The model keeps **no scrollback** (`scrollback_limit(0)`): a snapshot is the visible grid only, and
the ring already holds the raw history.

**`snapshot()` is `None` until `resize` has sized the model.** A snapshot serialized at a guessed grid
would render wrong, so the caller refuses instead (see the sequence point below). Every PTY spawner
calls `resize(rows, cols)` at spawn, before the child can write.

`append` feeds the model with **incremental UTF-8 decoding**: a code point split across two chunks is
held (at most three bytes) until the next chunk completes it, and bytes that are not UTF-8 render as
U+FFFD, as a terminal would. `avt` parses escape sequences across chunk boundaries itself.

### The dump rewrite (`src/screen_dump.rs`)

`portable_dump` rewrites `avt`'s dump into the dialect every client accepts:

- C1 CSI (`0x9b`) is written as `ESC [`.
- `?1047` (alternate screen without cursor save) is written as `?1049`.
- **Default-pen blank runs are skipped with cursor-forward** rather than printed. `avt` cannot tell a
  blank cell the application wrote a space into from one it never touched, and a client that
  distinguishes the two (copy and select do, and so does the reference emulator the snapshot tests
  compare against) would otherwise show untouched cells as written. A lone blank between two pieces
  of content is kept as a space — far more likely a word gap than an untouched cell. Blanks with a
  non-default pen are always printed, since their colour is visible.

## The sequence point

A snapshot is only useful paired with a live subscription that starts exactly where it ends. The
capture lock is that sequence point:

- **Producers append and broadcast under the lock.** `TaskChannel::write` locks the capture, calls
  `append`, sends the chunk on the stdout broadcast, then unlocks. `tddy-daemon-sandbox`'s stdio loop
  does the same.
- **Readers snapshot and subscribe under the lock.** `tddy_terminal_rpc::TerminalSession::snapshot_and_subscribe`
  locks the capture, takes `snapshot()`, subscribes to the broadcast, then unlocks.

So a chunk is either reflected in the snapshot or delivered on the subscription — never both, never
neither. `tests/terminal_capture_sequence_point.rs` pins it under a concurrent producer.

## Resize contract

The model must interpret output at the size the application draws for, so every path that resizes a
PTY resizes the capture first: `tddy_pty`'s spawn and registry `resize`, and
`tddy-cli-sessions`'s `PtyHandle::resize`. The model goes first because the child redraws as soon as
the PTY resizes, and that output must be read at the new grid. `grid() -> Option<(u16, u16)>` reports
the model's `(rows, cols)`, `None` while unsized.

## API

| Member | Meaning |
|---|---|
| `CAPTURE_LIMIT_BYTES` | 64 KiB — the retained-output bound. |
| `MAX_SEQUENCE_CHASE_BYTES` | 1 KiB — how far past the limit eviction may chase the end of a cut sequence. |
| `append(&[u8])` | Feed the screen model, sniff modes, extend the ring, evict down to the limit. |
| `resize(rows, cols)` | Size the screen model to the PTY's grid. |
| `grid() -> Option<(u16, u16)>` | The model's `(rows, cols)`; `None` until `resize`. |
| `snapshot() -> Option<ScreenSnapshot>` | The visible state as one repaint at `end_offset()`; `None` until `resize`. |
| `replay() -> Vec<u8>` | `mode_prologue()` ++ retained output — the legacy `TAIL` raw replay. |
| `mode_prologue() -> Vec<u8>` | `ESC[?<mode>h` per enabled mouse mode, ascending — puts a fresh VT back into the mouse modes now in effect. |
| `buffered_bytes() -> &[u8]` | Retained output only, no prologue. |

`mode_prologue` replays only the mouse family: `1000` (normal tracking), `1002` (button-event),
`1003` (any-event), and the `1005` / `1006` / `1015` / `1016` encodings. It precedes a raw byte tail,
and re-issuing other sticky modes — the alternate screen, bracketed paste — in front of bytes drawn
against an unknown state would fight the application's own; `terminal_capture_replay`'s
`ignores_private_modes_outside_the_mouse_tracking_family` pins that. So a legacy `TAIL` /
`FROM_OFFSET` attach is not told the program is on the alternate screen (backlog:
`2026-10-24-reattach-does-not-replay-the-alternate-screen`); a snapshot is.

## Escape-aware trimming

`EscapeParser` is a small VT state machine (Ground / Escape / EscapeIntermediate / CSI /
StringPayload / StringPayloadEscape) fed **one byte at a time**, so a DECSET split across two PTY
reads is still recognised.

Eviction uses it too: after trimming to the limit, it keeps going to the end of a sequence the cut
landed inside. Otherwise a cut through `ESC[1m` would leave a bare `1m` at the head of the replay,
which the client renders as literal text.

That chase is bounded by `MAX_SEQUENCE_CHASE_BYTES` (1 KiB). A sequence a client can render is at
most a few dozen bytes, so the bound is only ever reached by a payload the application never
terminated — an OSC missing its `ST`, say. Unbounded, the chase would run to the end of the buffer
and empty the ring, leaving a late subscriber a blank screen; giving up leaves a fragment instead,
which is the better trade. The ring therefore never holds less than
`CAPTURE_LIMIT_BYTES - MAX_SEQUENCE_CHASE_BYTES`.

The parser also recognises RIS (`ESC c`): a hard terminal reset turns mouse reporting off, so
`feed` reports `StreamEvent::FullReset` and the sticky mode state is cleared. Replaying modes past a
reset would leave a late subscriber's VT reporting events the application is no longer reading.

## `replay_capture()` is not `replay()`

`TaskChannel::replay_capture()` returns the process's own bytes, byte-exact, **without** a prologue.
Non-terminal consumers (`tddy-actions`, `tddy-build`, `tddy-core` session actions, the daemon's
`WatchTask`, `tddy-tool-engine`) treat that value as *command output* and would break if escape
bytes they never produced were prefixed.

Terminal callers take `capture_arc().lock().replay()` instead. The split is pinned by
`a_task_channel_replay_carries_only_what_the_process_wrote` in
`tests/terminal_capture_replay.rs`.

Every capture write flows through `TaskChannel::write`, so neither the sniffer nor the screen model
can be bypassed.

## Users outside `TaskChannel`

`tddy-daemon-sandbox`'s `sandbox_session.rs` holds its own `Arc<Mutex<TerminalCapture>>` (a jailed
session's PTY output arrives over a stdio bridge, not through a `TaskChannel`). It calls `append` on
every chunk and broadcasts it while still holding the lock (the [sequence point](#the-sequence-point)),
and `tddy-daemon`'s `terminal_session_adapter.rs` binds that capture to
`tddy_terminal_rpc::session::TerminalSession`, so a jailed terminal replays through the same
`tddy_terminal_rpc::bridge` arm as every other attach path — bounded, prologue first, anchored on the
same absolute offsets.

The daemon does not know a jailed PTY's size, so that capture is never `resize`d: its `snapshot()` is
`None` and a `SNAPSHOT` open of a jailed terminal is refused with `FAILED_PRECONDITION`. Jail sizing
is `#term-snapshot` 3/4 ([#762](https://github.com/uppin/tddy-coder/pull/762)).

## Known limits

- **CPU.** Every output byte also runs through the emulator. Measured: ~1 MB of synthetic TUI diff
  output (4 KiB chunks, 24×80, release build, Apple silicon) appends in 17.5 ms, ~57 MB/s; a snapshot
  of that screen serializes in ~0.1 ms (1.6 KB). Not yet measured on a recorded agent session.
- **Written blank vs untouched cell.** `avt` cannot tell them apart; the dump rewrite's heuristic
  (skip default-pen runs, keep a lone interior blank) is what keeps the snapshot equal to the
  reference emulator.

## Tests

| Suite | What it pins |
|---|---|
| `tests/terminal_capture_snapshot.rs` | snapshot fidelity against an independent reference emulator (`vt100`, dev-dependency): an alternate-screen TUI with scroll region, saved cursor, truecolor and input modes past 64 KiB of diffs; scroll region and saved cursor survive further bytes; a chunk boundary at every byte of a CSI and a 4-byte code point; a resize re-serializes at the new grid; an unsized capture has no snapshot |
| `tests/terminal_capture_sequence_point.rs` | `snapshot_and_subscribe` neither loses nor duplicates a byte under a concurrent producer |
| `tests/terminal_capture_replay.rs` | the mode prologue, eviction and the `replay_capture` / `replay` split |

## See also

- [tddy-terminal-rpc terminal-session-service.md § Snapshot opens](../../tddy-terminal-rpc/docs/terminal-session-service.md#snapshot-opens)

- [tddy-terminal-rpc terminal-session-service.md § The replay contract](../../tddy-terminal-rpc/docs/terminal-session-service.md#the-replay-contract)
- [tddy-daemon connection-service.md § Session terminals](../../tddy-daemon/docs/connection-service.md#session-terminals)
- [Web terminal § Touch/mouse mode](../../../docs/ft/web/web-terminal.md)
