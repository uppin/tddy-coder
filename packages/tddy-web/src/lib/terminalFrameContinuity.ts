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
 * Shared by the host feed and (in `#term-snapshot 4/4`) the room feed.
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
 * Judges each frame of one stream in order. A snapshot always paints and sets where the next frame
 * must start; after a gap every delta is a gap until the next snapshot, because none of them was
 * computed against the screen the client is showing.
 */
export class TerminalFrameContinuity {
  accept(frame: OffsetStampedFrame): ContinuityVerdict {
    // TODO(resync): implement
    void frame;
    return "paint";
  }
}
