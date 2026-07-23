/**
 * Chunking codec for the LiveKit RPC transport — the TypeScript mirror of the Rust codec at
 * `packages/tddy-livekit/src/chunking.rs`. The two MUST stay byte-for-byte compatible: a frame
 * split on one side is reassembled on the other.
 *
 * LiveKit's SCTP data channel negotiates a maximum message size (~64 KB); a single `publishData`
 * larger than that is rejected, so an oversized RPC envelope (e.g. a `StreamSessionActivity`
 * snapshot with a huge tool result) can never be delivered and the transport wedges. This module
 * splits an encoded envelope into ordered wire frames that each fit the per-packet budget, and
 * reassembles received frames back into the original bytes.
 *
 * Wire format & back-compat: a payload that already fits in one packet is sent RAW (byte-for-byte
 * the encoded envelope, exactly as before chunking existed), so a peer predating this codec still
 * decodes small messages. Only an oversized payload is split; each of its frames begins with a
 * one-byte {@link CHUNK_FRAME_MAGIC} followed by the header
 * `[magic: u8 = 0x00][messageId: u32 LE][totalChunks: u32 LE][index: u32 LE][data...]`.
 *
 * A valid protobuf `RpcRequest`/`RpcResponse` can never begin with `0x00` (protobuf field number 0
 * is illegal, and the envelopes' lowest fields are `request_id`/`response_message` at field
 * numbers 1 and 2), so a leading `0x00` unambiguously marks a chunk frame — see {@link isChunkFrame}.
 */

/** Safe upper bound for a single LiveKit data-channel message; under the ~64 KB SCTP max with
 *  headroom for the frame header. */
export const MAX_CHUNK_FRAME_BYTES = 60_000;

/** Leading byte marking a frame as a chunk frame rather than a raw envelope. */
export const CHUNK_FRAME_MAGIC = 0x00;

/** Encoded length of the chunk header: `[magic: u8][messageId: u32][totalChunks: u32][index: u32]`. */
const HEADER_LEN = 13;

let messageIdCounter = 0;

/**
 * Allocate the next process-unique `messageId`. A `messageId` only needs to be unique among one
 * sender's concurrently in-flight messages (the receiver keeps a separate reassembler per sender);
 * wrap-around at `u32::MAX` is harmless. Note this is distinct from the envelope `requestId` — a
 * streaming request reuses one `requestId` across many published envelopes.
 */
export function nextMessageId(): number {
  const id = messageIdCounter;
  messageIdCounter = (messageIdCounter + 1) >>> 0;
  return id;
}

/** Payload bytes that fit in one frame of `maxFrameBytes` (the budget minus the header). */
export function maxDataBytesPerFrame(maxFrameBytes: number): number {
  return Math.max(maxFrameBytes - HEADER_LEN, 0);
}

/** True when `bytes` is a chunk frame (to be reassembled) rather than a raw envelope (to decode). */
export function isChunkFrame(bytes: Uint8Array): boolean {
  return bytes.length > 0 && bytes[0] === CHUNK_FRAME_MAGIC;
}

/**
 * Prepare `payload` for the transport under the standard {@link MAX_CHUNK_FRAME_BYTES} budget: a
 * payload that fits in one packet is returned RAW (one frame, no header) for wire compatibility
 * with pre-chunking peers; a larger one is split into ordered chunk frames. A short payload that
 * happens to begin with {@link CHUNK_FRAME_MAGIC} is forced through the framed path so the receiver
 * never misreads it (in practice `payload` is always an encoded envelope, which never leads with
 * the magic).
 */
export function frameForTransport(messageId: number, payload: Uint8Array): Uint8Array[] {
  const fitsRaw = payload.length <= MAX_CHUNK_FRAME_BYTES && payload[0] !== CHUNK_FRAME_MAGIC;
  return fitsRaw ? [payload] : splitIntoFrames(messageId, payload, MAX_CHUNK_FRAME_BYTES);
}

/**
 * Split `payload` into ordered chunk frames tagged with `messageId`; each frame's length is at most
 * `maxFrameBytes`. Every frame carries the magic-prefixed header. Prefer {@link frameForTransport}
 * on the send path, which sends small payloads raw for back-compat.
 */
export function splitIntoFrames(
  messageId: number,
  payload: Uint8Array,
  maxFrameBytes: number,
): Uint8Array[] {
  const dataBudget = Math.max(maxDataBytesPerFrame(maxFrameBytes), 1);

  // An empty payload still yields a single (header-only) frame; treat it as one empty chunk.
  const dataChunks: Uint8Array[] = [];
  if (payload.length === 0) {
    dataChunks.push(payload.subarray(0, 0));
  } else {
    for (let offset = 0; offset < payload.length; offset += dataBudget) {
      dataChunks.push(payload.subarray(offset, Math.min(offset + dataBudget, payload.length)));
    }
  }
  const totalChunks = dataChunks.length;

  return dataChunks.map((data, index) => {
    const frame = new Uint8Array(HEADER_LEN + data.length);
    const view = new DataView(frame.buffer);
    frame[0] = CHUNK_FRAME_MAGIC;
    view.setUint32(1, messageId >>> 0, true);
    view.setUint32(5, totalChunks >>> 0, true);
    view.setUint32(9, index >>> 0, true);
    frame.set(data, HEADER_LEN);
    return frame;
  });
}

/** Thrown when a frame cannot be parsed as a chunk frame (too short, wrong magic, or missing chunk). */
export class ChunkError extends Error {
  constructor(message: string) {
    super(`malformed chunk frame: ${message}`);
    this.name = "ChunkError";
  }
}

interface PendingMessage {
  totalChunks: number;
  chunks: Map<number, Uint8Array>;
}

/**
 * Stateful reassembler. Feed each received chunk frame (those for which {@link isChunkFrame} is
 * true) to {@link accept}; it returns the completed payload once the final frame of a message
 * arrives, or `null` while chunks are still outstanding. Concurrent messages (distinct
 * `messageId`s) reassemble independently.
 */
export class ChunkReassembler {
  private readonly pending = new Map<number, PendingMessage>();

  /**
   * Accept one received chunk frame. Returns the completed payload when its message's final chunk
   * arrives, otherwise `null`. Throws {@link ChunkError} on a malformed frame.
   */
  accept(frame: Uint8Array): Uint8Array | null {
    if (frame.length < HEADER_LEN) {
      throw new ChunkError(`frame is ${frame.length} bytes, shorter than the ${HEADER_LEN}-byte header`);
    }
    if (frame[0] !== CHUNK_FRAME_MAGIC) {
      throw new ChunkError(
        `frame does not begin with the chunk magic (0x${frame[0].toString(16)} != 0x00)`,
      );
    }

    const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
    const messageId = view.getUint32(1, true);
    const totalChunks = view.getUint32(5, true);
    const index = view.getUint32(9, true);
    const data = frame.slice(HEADER_LEN);

    let entry = this.pending.get(messageId);
    if (!entry) {
      entry = { totalChunks, chunks: new Map() };
      this.pending.set(messageId, entry);
    }
    entry.chunks.set(index, data);

    if (entry.chunks.size < entry.totalChunks) {
      return null;
    }

    this.pending.delete(messageId);
    const parts: Uint8Array[] = [];
    let totalLength = 0;
    for (let i = 0; i < entry.totalChunks; i++) {
      const chunk = entry.chunks.get(i);
      if (chunk === undefined) {
        throw new ChunkError(`message ${messageId} is missing chunk ${i}`);
      }
      parts.push(chunk);
      totalLength += chunk.length;
    }
    const payload = new Uint8Array(totalLength);
    let offset = 0;
    for (const part of parts) {
      payload.set(part, offset);
      offset += part.length;
    }
    return payload;
  }
}
