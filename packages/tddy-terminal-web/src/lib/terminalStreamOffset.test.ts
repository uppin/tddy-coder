/**
 * Unit tests — `TerminalStreamOffset` (cumulative output-offset accounting for a terminal stream).
 *
 * The client tracks how many bytes of a terminal's cumulative output stream it has received so it can
 * reconnect with `StreamReplayMode.FROM_OFFSET` and receive only the gap. Getting that number wrong
 * is not a cosmetic bug: a counter that runs AHEAD of the real tip makes the daemon replay nothing on
 * every later reconnect, so the missed bytes — including whole redraw sequences — are dropped, and the
 * terminal renders half-consumed escape sequences over stale cells.
 *
 * Frames arrive in one order per stream open: the re-issued mode prologue (replayed VT state, zeroed
 * offsets), then the offset-anchored replay/catch-up frame, then live tail frames (zeroed offsets,
 * contiguous with the stream). Only the last two are stream bytes.
 *
 * Contract pinned here: pre-anchor frames are out-of-band and never move the counter.
 */

import { describe, expect, it } from "bun:test";
import { TerminalStreamOffset } from "./terminalStreamOffset";

/** The DECSETs a mouse-tracking TUI gets re-issued on every stream open (never stream bytes). */
const MODE_PROLOGUE = new TextEncoder().encode("\x1b[?1002h");

/** An offset-anchored replay/catch-up frame: absolute `endOffset` is the authority. */
function anAnchoredFrame(text: string, endOffset: bigint) {
  return { data: new TextEncoder().encode(text), endOffset };
}

/** A live tail frame: contiguous with the stream, carries no offset metadata. */
function aLiveFrame(text: string) {
  return { data: new TextEncoder().encode(text), endOffset: 0n };
}

/** The out-of-band mode prologue frame the bridge sends before the anchor. */
function aModePrologueFrame() {
  return { data: MODE_PROLOGUE, endOffset: 0n };
}

describe("TerminalStreamOffset", () => {
  it("carries the offset from the previous stream into a reconnect", () => {
    // Given — a terminal that had received 1024 bytes before the transport blipped
    // When
    const offset = new TerminalStreamOffset(1024n);

    // Then
    expect(offset.receivedUpTo).toEqual(1024n);
  });

  it("snaps to the absolute end offset of an offset-anchored replay frame", () => {
    // Given — a fresh stream whose client holds nothing yet
    const offset = new TerminalStreamOffset(0n);

    // When — the daemon replays the last screen, tagged as bytes 36..40 of the stream
    const receivedUpTo = offset.accept(anAnchoredFrame("tail", 40n));

    // Then — the server's absolute offset is the authority, not the frame's byte length
    expect(receivedUpTo).toEqual(40n);
  });

  it("advances by the byte length of a live tail frame once anchored", () => {
    // Given — a stream anchored at the tip (40)
    const offset = new TerminalStreamOffset(0n);
    offset.accept(anAnchoredFrame("tail", 40n));

    // When — three live bytes arrive
    const receivedUpTo = offset.accept(aLiveFrame("abc"));

    // Then
    expect(receivedUpTo).toEqual(43n);
  });

  it("ignores the mode prologue that precedes the anchor on a reconnect", () => {
    // Given — a client resuming at the tip (40); the bridge re-issues the 8-byte mode prologue first
    const offset = new TerminalStreamOffset(40n);

    // When
    const receivedUpTo = offset.accept(aModePrologueFrame());

    // Then — the prologue is replayed VT state, not stream bytes: the counter stays at the tip
    expect(receivedUpTo).toEqual(40n);
  });

  it("keeps the offset exact across repeated reconnects that re-issue the prologue", () => {
    // Given — a client at the tip (40) that reconnects three times with no new output in between.
    // Each open re-issues the prologue and re-anchors at the unchanged tip.
    const offset = new TerminalStreamOffset(40n);

    // When
    offset.accept(aModePrologueFrame());
    offset.accept(anAnchoredFrame("", 40n));
    offset.accept(aModePrologueFrame());
    offset.accept(anAnchoredFrame("", 40n));
    offset.accept(aModePrologueFrame());
    const receivedUpTo = offset.accept(anAnchoredFrame("", 40n));

    // Then — no drift accumulates. Counting each prologue as stream bytes would leave the client 24
    // bytes past the tip, and the daemon would replay nothing for the rest of the session.
    expect(receivedUpTo).toEqual(40n);
  });
});
