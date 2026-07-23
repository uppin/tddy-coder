/**
 * Unit tests for the TS chunking codec (`chunking.ts`), the mirror of the Rust codec in
 * `packages/tddy-livekit/src/chunking.rs`. These pin the wire format and reassembly behaviour that
 * must stay compatible across the TS <-> Rust boundary.
 */

import { describe, it, expect } from "bun:test";
import {
  MAX_CHUNK_FRAME_BYTES,
  CHUNK_FRAME_MAGIC,
  ChunkReassembler,
  ChunkError,
  frameForTransport,
  splitIntoFrames,
  isChunkFrame,
  maxDataBytesPerFrame,
  nextMessageId,
} from "./chunking.js";

/**
 * A deterministic payload of `len` bytes whose value varies per position (period 251, a prime that
 * won't align with chunk boundaries), so any dropped, duplicated, or reordered chunk changes the
 * reassembled bytes. Starts at 1 (never the chunk magic), like a real encoded envelope's leading tag.
 */
function aPayloadOf(len: number): Uint8Array {
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = (i % 251) + 1;
  return bytes;
}

/** A stand-in for a raw encoded `RpcResponse`: begins with `0x08`, the protobuf tag `prost` emits
 *  first for the `request_id` field. */
function anEncodedEnvelope(): Uint8Array {
  return new Uint8Array([0x08, 0x2a, 0x12, 0x03, 0x68, 0x69, 0x21]);
}

/** Feed frames into `reassembler`, returning every payload that completed (in completion order). */
function feedAll(reassembler: ChunkReassembler, frames: Uint8Array[]): Uint8Array[] {
  const completed: Uint8Array[] = [];
  for (const frame of frames) {
    const result = reassembler.accept(frame);
    if (result !== null) completed.push(result);
  }
  return completed;
}

/** Round-robins two frame sequences so chunks of both messages arrive intermixed. */
function interleave(a: Uint8Array[], b: Uint8Array[]): Uint8Array[] {
  const out: Uint8Array[] = [];
  const max = Math.max(a.length, b.length);
  for (let i = 0; i < max; i++) {
    if (i < a.length) out.push(a[i]);
    if (i < b.length) out.push(b[i]);
  }
  return out;
}

function expectBytesEqual(actual: Uint8Array, expected: Uint8Array): void {
  expect(Array.from(actual)).toEqual(Array.from(expected));
}

describe("chunking codec", () => {
  it("keeps a payload that already fits in a single frame", () => {
    // Given a payload well under the per-frame budget
    const payload = aPayloadOf(500);

    // When splitting it with a 60_000-byte budget
    const frames = splitIntoFrames(7, payload, 60_000);

    // Then it is not chunked
    expect(frames.length).toBe(1);
  });

  it("splits an oversized payload into frames that each fit the budget", () => {
    // Given a 370 KB payload — the size that overflowed LiveKit's 64 KB packet limit
    const payload = aPayloadOf(370_000);

    // When splitting it into 60_000-byte frames
    const frames = splitIntoFrames(1, payload, 60_000);

    // Then every frame fits within the budget
    for (const frame of frames) expect(frame.length).toBeLessThanOrEqual(60_000);
  });

  it("splits a payload into the expected number of frames", () => {
    // Given a payload just over two data-chunks long for a 1_000-byte budget
    const perFrame = maxDataBytesPerFrame(1_000);
    const payload = aPayloadOf(perFrame * 2 + 1);

    // When splitting it
    const frames = splitIntoFrames(1, payload, 1_000);

    // Then it produces exactly three frames
    expect(frames.length).toBe(3);
  });

  it("reassembles a split payload back into the original bytes", () => {
    // Given a 370 KB payload split into 60_000-byte frames
    const payload = aPayloadOf(370_000);
    const frames = splitIntoFrames(1, payload, 60_000);

    // When the frames are reassembled through a fresh reassembler
    const completed = feedAll(new ChunkReassembler(), frames);

    // Then the original bytes are recovered exactly
    expect(completed.length).toBe(1);
    expectBytesEqual(completed[0], payload);
  });

  it("yields nothing until the final chunk of a message arrives", () => {
    // Given a payload that splits into more than one frame
    const payload = aPayloadOf(2_500);
    const frames = splitIntoFrames(5, payload, 1_000);
    const reassembler = new ChunkReassembler();

    // When all but the final frame are fed in
    const beforeFinal = feedAll(reassembler, frames.slice(0, frames.length - 1));
    // And then the final frame arrives
    const onFinal = reassembler.accept(frames[frames.length - 1]);

    // Then nothing completed before the final frame, which completes the message
    expect(beforeFinal.length).toBe(0);
    expect(onFinal).not.toBeNull();
    expectBytesEqual(onFinal as Uint8Array, payload);
  });

  it("reassembles two interleaved messages independently", () => {
    // Given two distinct payloads chunked under a small budget
    const first = aPayloadOf(2_500);
    const second = aPayloadOf(3_100);
    const interleaved = interleave(
      splitIntoFrames(11, first, 1_000),
      splitIntoFrames(22, second, 1_000),
    );

    // When the interleaved frames are all fed to one reassembler
    const completed = feedAll(new ChunkReassembler(), interleaved);

    // Then both messages reassemble to their own original bytes
    expect(completed.length).toBe(2);
    expect(completed.some((c) => c.length === first.length && c[0] === first[0])).toBe(true);
    expect(completed.some((c) => c.length === second.length)).toBe(true);
  });

  it("rejects a frame too short to contain a header", () => {
    // Given a frame with fewer bytes than the chunk header requires
    const truncated = new Uint8Array([CHUNK_FRAME_MAGIC]);

    // When it is fed to the reassembler / Then it is rejected as malformed
    expect(() => new ChunkReassembler().accept(truncated)).toThrow(ChunkError);
  });

  it("rejects a frame that lacks the chunk magic", () => {
    // Given a full-length frame whose leading byte is not the chunk magic
    const notAChunk = splitIntoFrames(1, aPayloadOf(2_000), 1_000)[0].slice();
    notAChunk[0] = 0x08;

    // When it is fed to the reassembler / Then it is rejected as malformed
    expect(() => new ChunkReassembler().accept(notAChunk)).toThrow(ChunkError);
  });

  it("detects split frames as chunk frames", () => {
    // Given the frames of an oversized payload
    const frames = splitIntoFrames(3, aPayloadOf(200_000), 60_000);

    // Then every frame is recognised as a chunk frame
    expect(frames.every((f) => isChunkFrame(f))).toBe(true);
  });

  it("does not detect a raw envelope as a chunk frame", () => {
    // Given the bytes of a raw encoded envelope / Then it is not mistaken for a chunk frame
    expect(isChunkFrame(anEncodedEnvelope())).toBe(false);
  });

  it("sends a payload that fits as a single raw frame", () => {
    // Given an encoded envelope that fits within one packet
    const envelope = anEncodedEnvelope();

    // When preparing it for the transport
    const frames = frameForTransport(1, envelope);

    // Then it is sent raw — one frame, unchanged bytes, no chunk header
    expect(frames.length).toBe(1);
    expectBytesEqual(frames[0], envelope);
    expect(isChunkFrame(frames[0])).toBe(false);
  });

  it("frames an oversized payload into detectable chunks that reassemble", () => {
    // Given a 370 KB envelope that overflows a single packet
    const payload = aPayloadOf(370_000);

    // When preparing it for the transport
    const frames = frameForTransport(9, payload);

    // Then it is split into fitting chunk frames that reassemble to the original
    expect(frames.length).toBeGreaterThan(1);
    for (const frame of frames) expect(frame.length).toBeLessThanOrEqual(MAX_CHUNK_FRAME_BYTES);
    expect(frames.every((f) => isChunkFrame(f))).toBe(true);
    const completed = feedAll(new ChunkReassembler(), frames);
    expect(completed.length).toBe(1);
    expectBytesEqual(completed[0], payload);
  });

  it("frames a small payload that would look like a chunk frame", () => {
    // Given a short payload whose first byte collides with the chunk magic
    const payload = new Uint8Array([CHUNK_FRAME_MAGIC, 1, 2, 3, 4]);

    // When preparing it for the transport
    const frames = frameForTransport(2, payload);

    // Then it is framed (not sent raw) so the receiver won't misread it, and it round-trips
    expect(isChunkFrame(frames[0])).toBe(true);
    const completed = feedAll(new ChunkReassembler(), frames);
    expect(completed.length).toBe(1);
    expectBytesEqual(completed[0], payload);
  });

  it("hands out distinct message ids", () => {
    // When several outbound message ids are allocated
    const ids = [nextMessageId(), nextMessageId(), nextMessageId()];

    // Then no two collide
    expect(new Set(ids).size).toBe(3);
  });
});
