/**
 * Unit tests for the LiveKitTransport `meter` option.
 *
 * Verifies that the transport calls meter.record("out", n) for outbound
 * payload bytes and meter.record("in", n) for inbound response bytes,
 * using exact wire-payload sizes (not re-serialized approximations).
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { describe, it, expect, beforeEach, mock } from "bun:test";
import { Code, ConnectError } from "@connectrpc/connect";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import { TimestampSchema } from "@bufbuild/protobuf/wkt";
import { RoomEvent } from "livekit-client";
import {
  RpcRequestSchema,
  RpcResponseSchema,
  CallMetadataSchema,
} from "./gen/rpc_envelope_pb.js";
import { CHUNK_FRAME_MAGIC, splitIntoFrames } from "./chunking.js";
import { LiveKitTransport } from "./transport.js";

// ---------------------------------------------------------------------------
// Minimal fake Room
// ---------------------------------------------------------------------------

/** livekit-client passes `undefined` for the participant whenever the sender is not (yet) in the
 *  room's `remoteParticipants` map, so the identity is genuinely optional here. */
type DataReceivedListener = (
  payload: Uint8Array,
  participant?: { identity: string } | null,
  _kind?: unknown,
  topic?: string,
) => void;

function makeFakeRoom(onPublish?: (payload: Uint8Array) => void) {
  const listeners: Record<string, DataReceivedListener[]> = {};
  let lastPublishedPayload: Uint8Array | null = null;

  const room = {
    on(event: string, handler: DataReceivedListener) {
      listeners[event] = listeners[event] ?? [];
      listeners[event].push(handler);
      return room;
    },
    off(_event: string, _handler: DataReceivedListener) {
      return room;
    },
    localParticipant: {
      identity: "test-client",
      publishData(payload: Uint8Array, _opts: unknown) {
        lastPublishedPayload = payload;
        onPublish?.(payload);
      },
    },
    /** Test helper: emit a DataReceived event with the given payload. */
    _emit(event: string, payload: Uint8Array, participant: { identity: string } | null | undefined, topic: string) {
      for (const handler of listeners[event] ?? []) {
        handler(payload, participant, undefined, topic);
      }
    },
    _lastPublished() {
      return lastPublishedPayload;
    },
  };

  return room;
}

/** Build a valid RpcResponse binary for a given requestId with a small payload. */
function makeResponsePayload(requestId: number, responseMessageBytes: Uint8Array): Uint8Array {
  const response = create(RpcResponseSchema, {
    requestId,
    responseMessage: responseMessageBytes,
    endOfStream: false,
  });
  return toBinary(RpcResponseSchema, response);
}

// ---------------------------------------------------------------------------
// Fake method descriptor using real generated schemas
// (RpcRequestSchema / RpcResponseSchema are already available in this package)
// ---------------------------------------------------------------------------

const FAKE_METHOD = {
  kind: "unary",
  name: "Ping",
  parent: { typeName: "test.TestService" },
  input: RpcRequestSchema,
  output: RpcResponseSchema,
} as any;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("LiveKitTransport meter option", () => {
  it("calls meter.record('out', n) with the exact outbound payload byte length", async () => {
    const recorded: Array<{ dir: "in" | "out"; bytes: number }> = [];
    const meter = { record: (dir: "in" | "out", bytes: number) => recorded.push({ dir, bytes }) };

    let publishedPayload: Uint8Array | null = null;
    const fakeRoom = makeFakeRoom((payload) => { publishedPayload = payload; });

    const transport = new LiveKitTransport({
      room: fakeRoom as any,
      targetIdentity: "server",
      meter,
    } as any);

    // Start a unary call (will pend waiting for a response)
    const controller = new AbortController();
    const callPromise = transport.unary(
      FAKE_METHOD,
      controller.signal,
      undefined,
      undefined,
      {},
    );

    // Give the synchronous publishData call time to run
    await Promise.resolve();

    // Assert outbound was metered
    expect(recorded.some((r) => r.dir === "out" && r.bytes > 0)).toBe(true);

    // The outbound count must equal the actual published payload length
    const outRecord = recorded.find((r) => r.dir === "out");
    expect(outRecord?.bytes).toBe(publishedPayload?.length ?? -1);

    // Clean up — abort the pending call
    controller.abort();
    await callPromise.catch(() => {});
  });

  it("calls meter.record('in', n) with the exact inbound response payload byte length", async () => {
    const recorded: Array<{ dir: "in" | "out"; bytes: number }> = [];
    const meter = { record: (dir: "in" | "out", bytes: number) => recorded.push({ dir, bytes }) };

    // Capture the requestId synchronously from publishData so we can build the matching response.
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      try {
        capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
      } catch { /* ignore on parse failure */ }
    });

    const transport = new LiveKitTransport({
      room: fakeRoom as any,
      targetIdentity: "server",
      meter,
    } as any);

    // Start the unary call — publishData is called synchronously inside unary()
    const callPromise = transport.unary(FAKE_METHOD, undefined, undefined, undefined, {});

    // Wait one tick for publishData to have run
    await Promise.resolve();

    // Build a matching response and emit it
    const fakeMessageBytes = new Uint8Array([0x0a, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f]); // 7 bytes
    const responsePayload = makeResponsePayload(capturedRequestId, fakeMessageBytes);
    fakeRoom._emit(RoomEvent.DataReceived, responsePayload, { identity: "server" }, "tddy-rpc");

    // The call should now resolve
    await callPromise.catch(() => {});

    // The inbound record should reflect the full DataReceived payload length
    const inRecord = recorded.find((r) => r.dir === "in");
    expect(inRecord).toBeDefined();
    expect(inRecord?.bytes).toBe(responsePayload.length);
  });

  it("does not call meter when meter option is not provided", async () => {
    // Verifies backward-compat: no meter → no error
    const fakeRoom = makeFakeRoom();

    expect(() => {
      new LiveKitTransport({
        room: fakeRoom as any,
        targetIdentity: "server",
        // no meter
      });
    }).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// A method whose response message carries a 64-bit integer field.
//
// `google.protobuf.Timestamp.seconds` is an int64, which protobuf-es decodes to a JS `BigInt` — the
// same shape as the daemon's `GetHostDiskStatsResponse.available_bytes` (uint64) and
// `SessionEntry.bytes_in/out` (uint64). Using it here lets this in-package test exercise a
// BigInt-bearing response without importing the daemon's protos.
// ---------------------------------------------------------------------------

const FAKE_METHOD_RETURNING_TIMESTAMP = {
  kind: "unary",
  name: "GetTime",
  parent: { typeName: "test.TestService" },
  input: RpcRequestSchema,
  output: TimestampSchema,
} as any;

/** A valid RpcResponse for `requestId` wrapping a `Timestamp` whose int64 `seconds` = `seconds`. */
function aTimestampResponse(requestId: number, seconds: bigint): Uint8Array {
  const message = toBinary(TimestampSchema, create(TimestampSchema, { seconds }));
  return makeResponsePayload(requestId, message);
}

describe("LiveKitTransport unary — responses carrying 64-bit integer fields", () => {
  it("resolves a response whose message has a 64-bit integer field when debug logging is enabled", async () => {
    // Given — a transport with debug logging enabled (the condition under which the response is
    // logged), about to receive a message carrying a 64-bit integer field (decodes to a BigInt).
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
    });
    const transport = new LiveKitTransport({
      room: fakeRoom as any,
      targetIdentity: "server",
      debug: true,
    } as any);

    // When — the call is made and the server replies with seconds = 1_700_000_000 (a BigInt)
    const callPromise = transport.unary(FAKE_METHOD_RETURNING_TIMESTAMP, undefined, undefined, undefined, {});
    await Promise.resolve();
    fakeRoom._emit(
      RoomEvent.DataReceived,
      aTimestampResponse(capturedRequestId, 1_700_000_000n),
      { identity: "server" },
      "tddy-rpc",
    );

    // Then — the call resolves with the 64-bit value intact (debug logging must not throw on BigInt)
    const result = await callPromise;
    expect(result.message.seconds).toBe(1_700_000_000n);
  });
});

describe("LiveKitTransport unary — call deadlines", () => {
  it("rejects with DeadlineExceeded when no response arrives within timeoutMs", async () => {
    // Given — a request that is never answered. This is not hypothetical: a chunk-framed request
    // whose frames are dropped in transit leaves the peer's reassembler permanently incomplete, so
    // no response is ever produced (see `chunking.ts`).
    const fakeRoom = makeFakeRoom();
    const transport = new LiveKitTransport({ room: fakeRoom as any, targetIdentity: "server" } as any);

    // When — the caller sets a 20 ms deadline
    const callPromise = transport.unary(FAKE_METHOD, undefined, 20, undefined, {});

    // Then — the call fails instead of hanging forever
    let error: unknown = null;
    await callPromise.catch((e) => {
      error = e;
    });
    expect(error).toBeInstanceOf(ConnectError);
    expect((error as ConnectError).code).toBe(Code.DeadlineExceeded);
  });

  it("resolves normally when the response arrives before timeoutMs", async () => {
    // Given
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
    });
    const transport = new LiveKitTransport({ room: fakeRoom as any, targetIdentity: "server" } as any);

    // When — a generous deadline and a prompt response
    const callPromise = transport.unary(FAKE_METHOD, undefined, 60_000, undefined, {});
    await Promise.resolve();
    fakeRoom._emit(
      RoomEvent.DataReceived,
      makeResponsePayload(capturedRequestId, new Uint8Array(0)),
      { identity: "server" },
      "tddy-rpc",
    );

    // Then
    const result = await callPromise;
    expect(result.message).toBeDefined();
  });

  it("does not time out a call made without a timeoutMs", async () => {
    // Given — the pre-existing behaviour for callers that pass no deadline
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
    });
    const transport = new LiveKitTransport({ room: fakeRoom as any, targetIdentity: "server" } as any);

    // When — the response arrives late (after a timer tick that would have fired a short deadline)
    const callPromise = transport.unary(FAKE_METHOD, undefined, undefined, undefined, {});
    await new Promise((resolve) => setTimeout(resolve, 40));
    fakeRoom._emit(
      RoomEvent.DataReceived,
      makeResponsePayload(capturedRequestId, new Uint8Array(0)),
      { identity: "server" },
      "tddy-rpc",
    );

    // Then — it still resolves
    const result = await callPromise;
    expect(result.message).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// Chunk frames from a second sender
//
// Every sender's `messageId` counter starts at 0 (`chunking.ts`, `chunking.rs`), so two senders —
// or one sender before and after a restart — hand out the same ids. The standalone transport must
// therefore never let another sender's frames land in its peer's message. A `undefined` participant
// is the case that matters: livekit-client passes it whenever the sender is not in the room's
// `remoteParticipants` map, so identity-based filtering alone does not keep senders apart.
// ---------------------------------------------------------------------------

const FAKE_METHOD_RETURNING_CALL_METADATA = {
  kind: "unary",
  name: "Describe",
  parent: { typeName: "test.TestService" },
  input: RpcRequestSchema,
  output: CallMetadataSchema,
} as any;

/** Long enough that its response envelope needs three chunk frames under a 200-byte budget — the
 *  small-scale stand-in for a `ListSessionsResponse` that outgrows one LiveKit packet. */
const A_LONG_SERVICE_NAME = "connection.ConnectionService".padEnd(400, ".");

/** The message id both senders' messages carry: each counter starts at 0, so ids collide. */
const REUSED_MESSAGE_ID = 7;

/** Per-frame budget used to chunk the test envelope; small so a modest payload spans three frames. */
const A_SMALL_FRAME_BUDGET = 200;

/** A valid `RpcResponse` for `requestId` carrying a `CallMetadata` message. */
function aCallMetadataResponse(requestId: number): Uint8Array {
  const message = toBinary(
    CallMetadataSchema,
    create(CallMetadataSchema, { service: A_LONG_SERVICE_NAME, method: "ListSessions" }),
  );
  return makeResponsePayload(requestId, message);
}

/** A chunk frame of another sender's message that reuses `REUSED_MESSAGE_ID`: same header and same
 *  data length as `frames[index]`, different bytes. */
function aFrameOfAnotherSendersMessage(envelopeLength: number, index: number): Uint8Array {
  const foreignPayload = new Uint8Array(envelopeLength).fill(0xff);
  return splitIntoFrames(REUSED_MESSAGE_ID, foreignPayload, A_SMALL_FRAME_BUDGET)[index];
}

describe("LiveKitTransport — chunk frames from another sender", () => {
  it("resolves a chunked response with its own peer's bytes when another sender reuses the message id", async () => {
    // Given — a pending unary whose response is chunked into three frames
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
    });
    const transport = new LiveKitTransport({ room: fakeRoom as any, targetIdentity: "server" } as any);
    // 100 ms deadline so a message that never completes fails the test promptly instead of hanging.
    const callPromise = transport.unary(FAKE_METHOD_RETURNING_CALL_METADATA, undefined, 100, undefined, {});
    await Promise.resolve();
    const envelope = aCallMetadataResponse(capturedRequestId);
    const frames = splitIntoFrames(REUSED_MESSAGE_ID, envelope, A_SMALL_FRAME_BUDGET);

    // When — the peer's first two frames arrive, then an unidentified sender's final frame for the
    // same message id, then the peer's own final frame
    fakeRoom._emit(RoomEvent.DataReceived, frames[0], { identity: "server" }, "tddy-rpc");
    fakeRoom._emit(RoomEvent.DataReceived, frames[1], { identity: "server" }, "tddy-rpc");
    fakeRoom._emit(
      RoomEvent.DataReceived,
      aFrameOfAnotherSendersMessage(envelope.length, 2),
      undefined,
      "tddy-rpc",
    );
    fakeRoom._emit(RoomEvent.DataReceived, frames[2], { identity: "server" }, "tddy-rpc");

    // Then — the call resolves with the peer's message, not a mix of the two senders' frames
    const result = await callPromise;
    expect(result.message.service).toBe(A_LONG_SERVICE_NAME);
    expect(result.message.method).toBe("ListSessions");
  });
});

describe("LiveKitTransport — undecodable inbound frames", () => {
  it("reports a payload that cannot be decoded as a response envelope", () => {
    // Given — a transport that reports transport-level failures to its host
    const reported: unknown[] = [];
    const fakeRoom = makeFakeRoom();
    new LiveKitTransport({
      room: fakeRoom as any,
      targetIdentity: "server",
      onTransportError: (error: unknown) => reported.push(error),
    } as any);

    // When — bytes arrive that are not a decodable `RpcResponse`: field 2 declares five bytes of
    // `response_message` but only one follows, the wire shape of a truncated or mixed-up payload
    fakeRoom._emit(RoomEvent.DataReceived, new Uint8Array([0x12, 0x05, 0x68]), { identity: "server" }, "tddy-rpc");

    // Then — the failure is surfaced rather than silently dropped
    expect(reported.length).toBe(1);
    expect(reported[0]).toBeInstanceOf(Error);
  });

  it("reports a chunk frame too short to carry a chunk header", () => {
    // Given — a transport that reports transport-level failures to its host
    const reported: unknown[] = [];
    const fakeRoom = makeFakeRoom();
    new LiveKitTransport({
      room: fakeRoom as any,
      targetIdentity: "server",
      onTransportError: (error: unknown) => reported.push(error),
    } as any);

    // When — a frame marked as chunked arrives with fewer bytes than the 13-byte header
    fakeRoom._emit(RoomEvent.DataReceived, new Uint8Array([CHUNK_FRAME_MAGIC, 0x01, 0x02]), { identity: "server" }, "tddy-rpc");

    // Then — the failure is surfaced rather than silently dropped
    expect(reported.length).toBe(1);
    expect(reported[0]).toBeInstanceOf(Error);
  });
});

describe("LiveKitTransport unary — empty successful responses", () => {
  it("resolves (does not throw) when the response carries a valid, empty (zero-byte) protobuf message", async () => {
    // Given — a real RpcResponse whose responseMessage is exactly what a message with every field
    // at its default (e.g. `ListSessionsResponse{ sessions: [] }`) serializes to: zero bytes. This
    // is a normal, successful protobuf encoding, not a missing/malformed response.
    let capturedRequestId = 0;
    const fakeRoom = makeFakeRoom((payload) => {
      capturedRequestId = fromBinary(RpcRequestSchema, payload).requestId;
    });
    const transport = new LiveKitTransport({ room: fakeRoom as any, targetIdentity: "server" } as any);

    // When
    const callPromise = transport.unary(FAKE_METHOD, undefined, undefined, undefined, {});
    await Promise.resolve();
    const responsePayload = makeResponsePayload(capturedRequestId, new Uint8Array(0));
    fakeRoom._emit(RoomEvent.DataReceived, responsePayload, { identity: "server" }, "tddy-rpc");

    // Then
    const result = await callPromise;
    expect(result.message).toBeDefined();
  });
});
