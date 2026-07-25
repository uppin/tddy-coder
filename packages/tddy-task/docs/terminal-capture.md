# Terminal capture (mode-aware replay ring)

`tddy_task::TerminalCapture` (`src/terminal_capture.rs`) is the replay buffer behind every PTY-backed
task. It is what a late subscriber — a browser opening a terminal stream, a reconnecting LiveKit
client — is shown so the terminal is not blank until the application next repaints.

It is a **byte ring plus sticky terminal-mode state**, not a plain `Vec<u8>`.

## Why the modes are tracked separately

A terminal only reports clicks, drags and scrolls once **its own VT instance** has observed a
mouse-tracking DECSET in its byte stream. An agent TUI emits those bytes **once, at startup**.

The ring retains the last `CAPTURE_LIMIT_BYTES` (64 KiB) and trims from the front, so after ~64 KiB
of output the startup DECSETs are gone. Nothing re-emits them: `PtyHandle::trigger_redraw()` sends
SIGWINCH, and SIGWINCH makes an application redraw its *content* — it does not re-issue private
modes. A client attaching after that point could therefore never learn the terminal was in
mouse-tracking mode, and silently dropped every mouse event while keyboard input kept working.

So the capture sniffs DECSET/DECRST as bytes pass through and remembers the mouse-tracking modes
currently in effect. That state costs no ring space, so it survives eviction indefinitely.

## API

| Member | Meaning |
|---|---|
| `CAPTURE_LIMIT_BYTES` | 64 KiB — the retained-output bound. |
| `append(&[u8])` | Sniff modes, extend the ring, evict down to the limit. |
| `replay() -> Vec<u8>` | `mode_prologue()` ++ retained output. **What terminal clients want.** |
| `mode_prologue() -> Vec<u8>` | `ESC[?<mode>h` per enabled mode, ascending — puts a fresh VT back into the modes now in effect. |
| `buffered_bytes() -> &[u8]` | Retained output only, no prologue. |

Only the mouse family is replayed: `1000` (normal tracking), `1002` (button-event), `1003`
(any-event), and the `1006` / `1015` / `1016` encodings. Other sticky private modes — cursor
visibility (25), alternate screen (1049), bracketed paste — are deliberately **not** replayed: the
application redraws them, and re-issuing them out of context would fight the application's own
state.

## Escape-aware trimming

`EscapeParser` is a small VT state machine (Ground / Escape / EscapeIntermediate / CSI /
StringPayload / StringPayloadEscape) fed **one byte at a time**, so a DECSET split across two PTY
reads is still recognised.

Eviction uses it too: after trimming to the limit, it keeps going to the end of a sequence the cut
landed inside. Otherwise a cut through `ESC[1m` would leave a bare `1m` at the head of the replay,
which the client renders as literal text.

One consequence: a cut inside an *unterminated* OSC/DCS payload can transiently empty the ring until
the terminator arrives. There is also no RIS (`ESC c`) handling — sticky mode state survives a hard
terminal reset.

## `replay_capture()` is not `replay()`

`TaskChannel::replay_capture()` returns the process's own bytes, byte-exact, **without** a prologue.
Non-terminal consumers (`tddy-actions`, `tddy-build`, `tddy-core` session actions, the daemon's
`WatchTask`, `tddy-tool-engine`) treat that value as *command output* and would break if escape
bytes they never produced were prefixed.

Terminal callers take `capture_arc().lock().replay()` instead. The split is pinned by
`a_task_channel_replay_carries_only_what_the_process_wrote` in
`tests/terminal_capture_replay.rs`.

Every capture write flows through `TaskChannel::write`, so the sniffer cannot be bypassed.

## Known gap

`tddy-daemon`'s `sandbox_session.rs` keeps its own raw, **unbounded** `Arc<Mutex<Vec<u8>>>` capture
(`extend_from_slice`, no trim) and so gets no prologue. Pre-existing; tracked as a follow-up.

## See also

- [tddy-daemon connection-service.md § Terminal mode replay](../../tddy-daemon/docs/connection-service.md#terminal-mode-replay-mouse-tracking)
- [Web terminal § Touch/mouse mode](../../../docs/ft/web/web-terminal.md)
