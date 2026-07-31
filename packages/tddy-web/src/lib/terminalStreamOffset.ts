/**
 * Cumulative output-offset accounting for one open of a terminal output stream.
 *
 * The client tracks how many bytes of a terminal's cumulative output stream it has received so a
 * reconnect can resume with `StreamReplayMode.FROM_OFFSET` and receive only the gap. A counter that
 * runs AHEAD of the real tip is unrecoverable on its own: the daemon then has nothing to replay from
 * that offset, so every later reconnect silently drops the missed bytes and the terminal renders
 * half-consumed escape sequences over stale cells.
 *
 * Frames arrive in one order per stream open:
 *   1. the re-issued mode prologue — replayed VT state, zeroed offsets, NOT part of the stream;
 *   2. the offset-anchored replay / catch-up frame(s) — absolute `endOffset` is the authority;
 *   3. live tail frames — zeroed offsets, contiguous with the stream, so they advance by their length.
 *
 * The ordering is the only way to tell (1) from (3), since both carry zeroed offsets: everything
 * before the first anchored frame is out-of-band and must not move the counter. The bridge always
 * emits exactly one anchored frame per open — an empty one tagged with the capture tip when there was
 * no gap to replay — so the counter is SET by the server on every open rather than inferred.
 *
 * One instance per stream open, seeded with the offset carried over from the previous stream.
 */
export class TerminalStreamOffset {
  private offset: bigint;
  /** Whether the offset-anchored frame for this open has arrived (frames before it are out-of-band). */
  private anchored = false;

  constructor(receivedUpTo: bigint) {
    this.offset = receivedUpTo;
  }

  /** The cumulative byte offset of the output stream the client has received up to. */
  get receivedUpTo(): bigint {
    return this.offset;
  }

  /** Account for one output frame, returning the offset received up to. */
  accept(frame: { data: Uint8Array; endOffset: bigint }): bigint {
    if (frame.endOffset > 0n) {
      // The anchor: the server's absolute offset replaces whatever the client had counted, which is
      // what corrects a counter that drifted (the bridge clamps it to the real capture tip).
      this.anchored = true;
      this.offset = frame.endOffset;
    } else if (this.anchored) {
      this.offset += BigInt(frame.data.length);
    }
    return this.offset;
  }
}
