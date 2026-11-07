/**
 * Offset-continuity guard for a terminal stream opened with a snapshot.
 *
 * After a snapshot, every frame the daemon sends is stamped with the offsets it occupies in the
 * terminal's cumulative output: the first delta starts at the snapshot's `endOffset`, and each
 * later one at the previous frame's `endOffset`. A frame that does not start where the last one
 * ended means bytes were lost or repeated somewhere on the way, and painting it would draw the
 * application's diffs over a screen they were not computed against. The guard says so, so a feed
 * can drop the frame and ask for a fresh snapshot instead.
 *
 * Used by both bidi host streams — `openDaemonTerminalFeed` and `GrpcSessionTerminal` — and (in
 * `#term-snapshot 4/4`) by the room feed.
 */

/** What a frame needs to be judged: its offsets, and whether it is a snapshot. */
export interface OffsetStampedFrame {
  readonly snapshot?: boolean;
  readonly startOffset?: bigint;
  readonly endOffset: bigint;
}

/** `paint` — the frame continues the screen. `gap` — it does not; drop it and resync. */
export type ContinuityVerdict = "paint" | "gap";

/**
 * What a feed does with a frame: `paint` it, `drop` it, or `drop-and-ask` — drop it and send one
 * `requestSnapshot`. A run of gapped frames asks once; the snapshot that answers re-arms the ask.
 */
export type FrameAction = "paint" | "drop" | "drop-and-ask";

/**
 * Judges each frame of one stream in order. A snapshot always paints and sets where the next frame
 * must start; after a gap every delta is a gap until the next snapshot, because none of them was
 * computed against the screen the client is showing.
 */
export class TerminalFrameContinuity {
  /** Where the next delta must start; `undefined` before a snapshot and after a gap. */
  private nextStart: bigint | undefined;
  /** A snapshot was asked for and has not arrived yet. */
  private snapshotAsked = false;

  /** `accept`, plus the once-per-gap snapshot request a feed sends on its open stream. */
  route(frame: OffsetStampedFrame): FrameAction {
    if (this.accept(frame) === "paint") {
      if (frame.snapshot) this.snapshotAsked = false;
      return "paint";
    }
    if (this.snapshotAsked) return "drop";
    this.snapshotAsked = true;
    return "drop-and-ask";
  }

  accept(frame: OffsetStampedFrame): ContinuityVerdict {
    if (frame.snapshot) {
      this.nextStart = frame.endOffset;
      return "paint";
    }
    if (this.nextStart === undefined || frame.startOffset !== this.nextStart) {
      this.nextStart = undefined;
      return "gap";
    }
    this.nextStart = frame.endOffset;
    return "paint";
  }
}
