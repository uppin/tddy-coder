# Enqueued Input Overlay & Input-Offset ACK

**Product Area**: Web (terminal)
**Status**: Implemented
**Updated**: 2026-07-25
**Related**: [web-terminal.md](web-terminal.md) (§ Connected Terminal UX), [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md)

## Summary

On a slow or lossy network the browser terminal feels laggy and inconsistent: keystrokes and
mouse events are sent fire-and-forget over `SendTerminalInput`, and the only visual confirmation
is the PTY echoing them back through `StreamTerminalOutput` — which may arrive hundreds of
milliseconds later, or reorder relative to what the user typed.

This feature makes terminal input **accountable end-to-end**:

1. Every byte the client sends is assigned a **cumulative input offset**. The offset of a chunk is
   the running total number of input bytes sent through this terminal **including** that chunk.
2. The daemon, after writing those bytes to the PTY master, **acknowledges the applied offset** by
   streaming it back on the existing `StreamTerminalOutput` stream (a new
   `acked_input_offset` field on `SessionTerminalOutput`).
3. The client keeps an **input queue** of un-acknowledged chunks. If the oldest un-acked byte is
   not acknowledged within **500 ms**, the client shows an **enqueued-input overlay** — a single
   line reflecting exactly what has been typed but not yet confirmed by the server.
4. As ACKs arrive, the overlay **collapses from the front**: acking the first 3 bytes of
   `HELLO WORLD` turns the overlay from `HELLO WORLD` into `LO WORLD`. When every byte is acked,
   the overlay disappears.

The result: on a fast link the overlay never appears (ACKs beat the 500 ms threshold); on a slow
link the user sees their own input immediately and watches it drain as the server catches up,
instead of staring at a frozen terminal.

## Background

Today (see [web-terminal.md](web-terminal.md)) all browser terminal input — keyboard, SGR mouse
sequences, and the synthetic `\x1b]resize;{cols};{rows}\x07` OSC — funnels through
`GhosttyTerminal.onData` → `sendInput` → `GrpcSessionTerminal.sendTerminalInput` (a **unary**
`ConnectionService.SendTerminalInput`). Output arrives on the separate server-streaming
`StreamTerminalOutput`. Neither message carries any sequence number, byte offset, or
acknowledgement:

- `SessionTerminalInput { session_token, session_id, data, terminal_id, control_token }`
- `SessionTerminalOutput { data }`
- `SendTerminalInputResponse {}` — empty; the unary response confirms only that the RPC was
  received, not that the bytes reached the PTY.

On the daemon, `send_terminal_input` resolves the `PtyHandle` and calls
`PtyHandle::send_input(data)` (`cli_session_manager.rs`), which forwards to the PTY writer thread
(`tddy-pty/src/runtime.rs`). That write is the authoritative "applied to the PTY" point, and is
where the ACK originates.

## Requirements

### Input offset (client)

1. `GrpcSessionTerminal` maintains a **monotonic cumulative byte counter** for the terminal, starting
   at 0. Each `send(data)` advances it by `data.length`; the value **after** advancing is the chunk's
   **input offset**.
2. `SendTerminalInput` carries this offset in a new field `input_offset` (proto `uint64`; generated
   TS `bigint`). Every send — keyboard, mouse, resize OSC, file-drop path insertion, mobile
   shortcut — participates in the same counter, so the offset is a faithful byte count of the
   input stream.
3. The counter is **per terminal** (`terminal_id`); Agent and bash terminals count independently.

### Input-offset ACK (daemon)

4. `SessionTerminalOutput` gains `acked_input_offset` (proto `uint64`). On a normal output-data
   frame it is `0` (unset); on an **ACK frame** it is the highest input offset the daemon has
   written to the PTY, and `data` is empty.
5. `send_terminal_input` passes `req.input_offset` through to `PtyHandle::send_input(data, offset)`.
   After the bytes are handed to the PTY writer, the handle records the applied offset as the
   **maximum** of the current value and `offset` (monotonic; never regresses) and publishes it.
6. `stream_terminal_output` interleaves ACK frames into the output stream: whenever the applied
   offset advances, it emits `SessionTerminalOutput { data: [], acked_input_offset: N }`. Ordering
   with data frames on that stream is preserved.
7. ACK reflects the **client's** byte accounting: the offset is echoed unchanged. Resize-OSC bytes
   that the daemon strips before writing to the PTY are still counted toward the acked offset (the
   client counted them, so the server acks them).

### Enqueued-input queue (client)

8. The client keeps an ordered queue of sent chunks, each recording `[startOffset, endOffset)`, the
   raw bytes, and a **classification**: `text`, `mouse`, or `control`.
   - `mouse` — the chunk matches a mouse report sequence (`\x1b[<…M`/`m`, or legacy `\x1b[M…`).
   - `text` — not mouse, and every byte is printable (`0x20`–`0x7e`) or a UTF-8 continuation/lead
     byte (`≥ 0x80`).
   - `control` — everything else (Enter/`\r`, arrows, escapes, backspace, the resize OSC).
9. An incoming `acked_input_offset` **N** marks bytes `[0, N)` as applied. The queue drops fully-acked
   chunks and trims the straddling chunk to its un-acked byte suffix. Acked offset only ever
   increases (max wins); a stale/lower ACK is ignored.
10. The queue is **transport-truth**: it is fed by the same `send()` the RPC uses and by the acks the
    stream delivers — it does not guess.

### Enqueued-input overlay (client)

11. The overlay is shown only when the **oldest un-acked byte** has been outstanding longer than the
    **overlay delay** (default **500 ms**). On a link where ACKs arrive sooner, the overlay never
    appears.
12. Once shown, the overlay stays while any byte is un-acked and **re-flows** as ACKs arrive; it hides
    as soon as the queue is empty (fully acked).
13. The overlay is a **single line**. Its content is built from the un-acked chunks, left to right:
    - a maximal run of `text` renders as that decoded text, inline;
    - a maximal run of `mouse` chunks collapses into **one** mouse glyph with the run count
      (e.g. `🖱×3`), so a flurry of mouse events never floods the line;
    - `control` chunks are **not** rendered (tracked for offset accounting only).
14. **Overflow**: the visible line holds at most `maxItems` display items (a text character is one
    item; a coalesced mouse run is one item). Items beyond the budget — the newest, right-most — are
    dropped from the line and their count is shown in a trailing **overflow glyph** (e.g. `⋯+4`).
    Acking the front reduces the un-acked set, so overflow shrinks and eventually clears.
15. The overlay is presentational and reflects queue state only; it never itself sends input or ACKs.

### Worked example (from the brief)

Typing `HELLO WORLD` (11 bytes) on a slow link, no ACK within 500 ms → overlay shows `HELLO WORLD`.
Server ACKs offset **3** (`HEL` applied) → overlay collapses to `LO WORLD`. Server ACKs offset
**11** → overlay disappears.

## Non-functional

- The 500 ms delay and `maxItems` budget are constants with named symbols, overridable via props for
  tests (deterministic clock control), not magic numbers scattered in the component.
- Offset arithmetic uses JS `number` internally (safe to 2^53 bytes) and converts to/from `bigint`
  only at the RPC boundary.
- No behavior change on a fast link: with prompt ACKs the overlay is never mounted and the queue
  drains continuously.

## Acceptance Criteria

### Proto & wire contract
- [x] `SessionTerminalInput.input_offset` (`uint64`) and `SessionTerminalOutput.acked_input_offset`
  (`uint64`) exist and are regenerated for Rust and TS.

### Input offset (web)
- [x] Consecutive `SendTerminalInput` calls carry `input_offset` equal to the running byte total:
  for every pair, `offset[i] == offset[i-1] + data[i].length`, and `offset[0] == data[0].length`.
  (`TerminalInputAckAcceptance.cy.tsx` Part A — a real-ghostty integration test; runs on a
  WebGL-capable CI runner. The cumulative-offset logic itself is unit-covered in
  `terminalInputQueue.test.ts`.)

### ACK (daemon & coder participant)
- [x] After `send_terminal_input` with `input_offset = N`, `stream_terminal_output` emits a frame
  with `acked_input_offset = N` and empty `data` (`terminal_input_ack_acceptance.rs`).
- [x] The applied offset is monotonic: a later input with a smaller offset never lowers the acked
  value.
- [x] The ACK state lives on the shared `tddy_task::TaskChannel`, so it works for both daemon-hosted
  terminals (claude-cli) and tddy-coder-participant-hosted terminals (bash tabs) — the daemon rebuilds
  a `PtyHandle` per RPC, so per-handle state would not be shared.

### Overlay behavior (web)
- [x] No ACK within 500 ms of typing → overlay appears showing the typed text.
- [x] An ACK of the first K bytes collapses the overlay to the un-acked suffix.
- [x] A full ACK hides the overlay.
- [x] Consecutive mouse events collapse into a single mouse glyph with the correct count.
- [x] Un-acked content beyond the single-line budget collapses into an overflow glyph with the
  correct overflow count.
- [x] A fast ACK (before 500 ms) never shows the overlay.

## Future Considerations (Not In Scope)

- Rich control-key glyphs in the overlay (render Enter/arrows/Ctrl-combos instead of omitting them).
- Applying the same offset/ACK protocol to the LiveKit bidi `StreamSessionTerminalIO` path and to
  `TerminalService` (VirtualTui).
- Client-side **resend** of un-acked input after a reconnect using the last acked offset as the
  resume point.
- Per-chunk latency surfaced as a small RTT indicator.
