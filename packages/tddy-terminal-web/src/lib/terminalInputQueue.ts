/**
 * Terminal input queue — client-side accounting of un-acknowledged terminal input.
 *
 * docs/ft/web/enqueued-input-overlay.md
 *
 * Every chunk the browser sends to the PTY is assigned a cumulative byte offset. The daemon acks
 * the highest offset it has applied (on StreamTerminalOutput); this queue drops acked bytes from
 * the front and exposes the un-acked remainder as a single-line overlay model — keyboard text
 * inline, a run of mouse events collapsed to one glyph with a count, and anything past the line
 * budget collapsed into an overflow count.
 */

/** How an input chunk is shown in the overlay. */
export type InputKind = "text" | "mouse" | "control";

/** One left-to-right piece of the single-line overlay. */
export type OverlaySegment =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "mouse"; readonly count: number };

/** The single-line overlay: ordered segments plus a count of items collapsed past the budget. */
export interface OverlayModel {
  readonly segments: OverlaySegment[];
  readonly overflowCount: number;
}

const ESC = 0x1b;
const BRACKET = 0x5b; // '['
const LESS_THAN = 0x3c; // '<'
const UPPER_M = 0x4d; // 'M'

/** SGR mouse report, e.g. `\x1b[<0;10;5M` (press) or `...m` (release). */
const SGR_MOUSE = /^\x1b\[<[0-9;]+[Mm]$/;

function isPrintableByte(b: number): boolean {
  // Printable ASCII or any UTF-8 lead/continuation byte; DEL (0x7f) and C0 controls are not.
  return (b >= 0x20 && b <= 0x7e) || b >= 0x80;
}

/**
 * Classify one input chunk as keyboard text, a mouse report, or a control sequence.
 *
 * Mouse: an SGR mouse report (`\x1b[<…M`/`m`) or a legacy X10 report (`\x1b[M…`).
 * Text: not mouse, and every byte is printable.
 * Control: everything else (Enter, arrows, escapes, backspace, the resize OSC).
 */
export function classifyInput(bytes: Uint8Array): InputKind {
  const isLegacyMouse =
    bytes.length >= 3 && bytes[0] === ESC && bytes[1] === BRACKET && bytes[2] === UPPER_M;
  if (isLegacyMouse) return "mouse";

  const isSgrMouse =
    bytes.length >= 4 &&
    bytes[0] === ESC &&
    bytes[1] === BRACKET &&
    bytes[2] === LESS_THAN &&
    SGR_MOUSE.test(new TextDecoder().decode(bytes));
  if (isSgrMouse) return "mouse";

  if (bytes.length > 0 && Array.from(bytes).every(isPrintableByte)) return "text";
  return "control";
}

interface InputChunk {
  readonly startOffset: number;
  readonly endOffset: number;
  readonly bytes: Uint8Array;
  readonly kind: InputKind;
}

/** One display unit before coalescing: a single text character or a coalesced mouse run. */
type DisplayItem = { kind: "text"; ch: string } | { kind: "mouse"; count: number };

const decoder = new TextDecoder();

export class TerminalInputQueue {
  /** Chunks not yet fully acked (fully-acked chunks are pruned on `ack`). */
  private chunks: InputChunk[] = [];
  private total = 0;
  private acked = 0;

  /** Append `bytes` to the input stream; returns the cumulative end offset assigned to this chunk. */
  enqueue(bytes: Uint8Array): number {
    const startOffset = this.total;
    const endOffset = startOffset + bytes.length;
    this.total = endOffset;
    this.chunks.push({ startOffset, endOffset, bytes, kind: classifyInput(bytes) });
    return endOffset;
  }

  /** Acknowledge that input bytes `[0, offset)` have been applied. Monotonic — a lower offset is ignored. */
  ack(offset: number): void {
    if (offset <= this.acked) return;
    this.acked = offset;
    this.chunks = this.chunks.filter((c) => c.endOffset > this.acked);
  }

  get ackedOffset(): number {
    return this.acked;
  }

  get totalOffset(): number {
    return this.total;
  }

  hasUnacked(): boolean {
    return this.acked < this.total;
  }

  /** Build the single-line overlay for the un-acked remainder, bounded to `maxItems` display items. */
  overlayModel(maxItems: number): OverlayModel {
    const items = this.displayItems();
    const visible = items.slice(0, maxItems);
    const overflowCount = items.length - visible.length;
    return { segments: coalesce(visible), overflowCount };
  }

  /** Expand the un-acked chunk suffixes into ordered display items (text per char, mouse per run). */
  private displayItems(): DisplayItem[] {
    const items: DisplayItem[] = [];
    for (const chunk of this.chunks) {
      // Trim the straddling chunk to the bytes past the acked boundary.
      const from = Math.max(0, this.acked - chunk.startOffset);
      const suffix = from > 0 ? chunk.bytes.subarray(from) : chunk.bytes;
      if (suffix.length === 0) continue;

      if (chunk.kind === "mouse") {
        const last = items[items.length - 1];
        if (last && last.kind === "mouse") {
          last.count += 1;
        } else {
          items.push({ kind: "mouse", count: 1 });
        }
      } else if (chunk.kind === "text") {
        for (const ch of decoder.decode(suffix)) {
          items.push({ kind: "text", ch });
        }
      }
      // control chunks are tracked for offset accounting only — not rendered.
    }
    return items;
  }
}

/** Merge adjacent text items into one text segment; each mouse item is its own segment. */
function coalesce(items: DisplayItem[]): OverlaySegment[] {
  const segments: OverlaySegment[] = [];
  let textRun = "";
  const flushText = () => {
    if (textRun.length > 0) {
      segments.push({ kind: "text", text: textRun });
      textRun = "";
    }
  };
  for (const item of items) {
    if (item.kind === "text") {
      textRun += item.ch;
    } else {
      flushText();
      segments.push({ kind: "mouse", count: item.count });
    }
  }
  flushText();
  return segments;
}
