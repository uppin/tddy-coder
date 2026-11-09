/**
 * Unit tests — terminal input queue (offset accounting, ACK trimming, overlay model).
 *
 * docs/ft/web/enqueued-input-overlay.md
 *
 * The queue is the client-side source of truth for un-acknowledged terminal input: every byte
 * sent gets a cumulative offset, ACKs collapse the queue from the front, and the overlay model
 * coalesces the un-acked remainder into a single-line view (text inline, mouse runs to one glyph,
 * items past the budget to an overflow count).
 */

import { describe, expect, it } from "bun:test";
import {
  classifyInput,
  TerminalInputQueue,
  type OverlayModel,
} from "./terminalInputQueue";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const bytes = (s: string): Uint8Array => new TextEncoder().encode(s);

/** A single SGR mouse report (press at col 10, row 5). */
const MOUSE = "\x1b[<0;10;5M";

const aQueue = () => new TerminalInputQueue();

/** Enqueue each character of `s` as its own 1-byte chunk (mirrors keystroke-by-keystroke input). */
function typeChars(queue: TerminalInputQueue, s: string): void {
  for (const ch of s) queue.enqueue(bytes(ch));
}

/** Concatenate the overlay model's text segments (mouse/overflow excluded). */
function overlayText(model: OverlayModel): string {
  return model.segments
    .filter((seg) => seg.kind === "text")
    .map((seg) => (seg.kind === "text" ? seg.text : ""))
    .join("");
}

// ---------------------------------------------------------------------------
// classifyInput
// ---------------------------------------------------------------------------

describe("classifyInput", () => {
  it("classifies a printable letter as text", () => {
    expect(classifyInput(bytes("H"))).toEqual("text");
  });

  it("classifies a space as text", () => {
    expect(classifyInput(bytes(" "))).toEqual("text");
  });

  it("classifies an SGR mouse press sequence as mouse", () => {
    expect(classifyInput(bytes("\x1b[<0;10;5M"))).toEqual("mouse");
  });

  it("classifies an SGR mouse release sequence as mouse", () => {
    expect(classifyInput(bytes("\x1b[<0;10;5m"))).toEqual("mouse");
  });

  it("classifies a carriage return as control", () => {
    expect(classifyInput(bytes("\r"))).toEqual("control");
  });

  it("classifies an arrow-key escape sequence as control", () => {
    expect(classifyInput(bytes("\x1b[A"))).toEqual("control");
  });
});

// ---------------------------------------------------------------------------
// Offset accounting
// ---------------------------------------------------------------------------

describe("TerminalInputQueue — offset accounting", () => {
  it("returns the cumulative end offset from enqueue", () => {
    // Given
    const queue = aQueue();

    // When
    const first = queue.enqueue(bytes("abc"));
    const second = queue.enqueue(bytes("de"));

    // Then — offsets are the running byte total after each chunk
    expect(first).toEqual(3);
    expect(second).toEqual(5);
    expect(queue.totalOffset).toEqual(5);
  });

  it("starts with nothing un-acked and a zero acked offset", () => {
    const queue = aQueue();
    expect(queue.hasUnacked()).toEqual(false);
    expect(queue.ackedOffset).toEqual(0);
  });
});

// ---------------------------------------------------------------------------
// ACK trimming
// ---------------------------------------------------------------------------

describe("TerminalInputQueue — ACK", () => {
  it("collapses the queue to the un-acked byte suffix when a prefix is acked", () => {
    // Given — HELLO WORLD typed key by key, nothing acked
    const queue = aQueue();
    typeChars(queue, "HELLO WORLD");

    // When — the server acks the first 3 bytes ("HEL")
    queue.ack(3);

    // Then
    expect(queue.ackedOffset).toEqual(3);
    expect(queue.hasUnacked()).toEqual(true);
    expect(overlayText(queue.overlayModel(40))).toEqual("LO WORLD");
  });

  it("has nothing un-acked once every byte is acked", () => {
    // Given
    const queue = aQueue();
    typeChars(queue, "HELLO WORLD");

    // When
    queue.ack(11);

    // Then
    expect(queue.hasUnacked()).toEqual(false);
    expect(queue.overlayModel(40).segments).toEqual([]);
  });

  it("is monotonic — a lower ack offset never lowers the acked value", () => {
    // Given
    const queue = aQueue();
    typeChars(queue, "HELLO WORLD");
    queue.ack(5);

    // When — a stale, lower ack arrives
    queue.ack(3);

    // Then
    expect(queue.ackedOffset).toEqual(5);
    expect(overlayText(queue.overlayModel(40))).toEqual(" WORLD");
  });

  it("trims a straddling multi-byte chunk to its un-acked suffix", () => {
    // Given — one 5-byte chunk, then a 3-byte chunk
    const queue = aQueue();
    queue.enqueue(bytes("HELLO"));
    queue.enqueue(bytes(" WO"));

    // When — ack lands inside the first chunk
    queue.ack(3);

    // Then — the partially-applied chunk contributes only its un-acked tail
    expect(overlayText(queue.overlayModel(40))).toEqual("LO WO");
  });
});

// ---------------------------------------------------------------------------
// Overlay model — coalescing & overflow
// ---------------------------------------------------------------------------

describe("TerminalInputQueue — overlay model", () => {
  it("coalesces consecutive text into a single segment", () => {
    // Given
    const queue = aQueue();
    typeChars(queue, "abc");

    // Then
    expect(queue.overlayModel(40).segments).toEqual([{ kind: "text", text: "abc" }]);
  });

  it("collapses a run of mouse events into one segment carrying the count", () => {
    // Given — three mouse events
    const queue = aQueue();
    queue.enqueue(bytes(MOUSE));
    queue.enqueue(bytes(MOUSE));
    queue.enqueue(bytes(MOUSE));

    // Then
    expect(queue.overlayModel(40).segments).toEqual([{ kind: "mouse", count: 3 }]);
  });

  it("keeps text and mouse runs in order", () => {
    // Given — "ab", a mouse event, then "cd"
    const queue = aQueue();
    typeChars(queue, "ab");
    queue.enqueue(bytes(MOUSE));
    typeChars(queue, "cd");

    // Then
    expect(queue.overlayModel(40).segments).toEqual([
      { kind: "text", text: "ab" },
      { kind: "mouse", count: 1 },
      { kind: "text", text: "cd" },
    ]);
  });

  it("omits control input from the visible line", () => {
    // Given — "a", Enter, "b": the Enter (control) is tracked for offset but not shown
    const queue = aQueue();
    queue.enqueue(bytes("a"));
    queue.enqueue(bytes("\r"));
    queue.enqueue(bytes("b"));

    // Then — only the printable text renders
    expect(overlayText(queue.overlayModel(40))).toEqual("ab");
  });

  it("collapses items beyond the budget into an overflow count", () => {
    // Given — six text items, a three-item line budget
    const queue = aQueue();
    typeChars(queue, "abcdef");

    // When
    const model = queue.overlayModel(3);

    // Then — the first three items show, the rest collapse into the overflow count
    expect(overlayText(model)).toEqual("abc");
    expect(model.overflowCount).toEqual(3);
  });

  it("counts a coalesced mouse run as a single line item for the budget", () => {
    // Given — two text items then a five-event mouse run; budget of three items
    const queue = aQueue();
    typeChars(queue, "ab");
    for (let i = 0; i < 5; i++) queue.enqueue(bytes(MOUSE));

    // When
    const model = queue.overlayModel(3);

    // Then — "a", "b", and the whole mouse run (one item) fit; nothing overflows
    expect(model.segments).toEqual([
      { kind: "text", text: "ab" },
      { kind: "mouse", count: 5 },
    ]);
    expect(model.overflowCount).toEqual(0);
  });
});
