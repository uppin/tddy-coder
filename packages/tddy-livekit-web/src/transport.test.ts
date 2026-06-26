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
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import { RoomEvent } from "livekit-client";
import {
  RpcRequestSchema,
  RpcResponseSchema,
  CallMetadataSchema,
} from "./gen/rpc_envelope_pb.js";
import { LiveKitTransport } from "./transport.js";

// ---------------------------------------------------------------------------
// Minimal fake Room
// ---------------------------------------------------------------------------

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
    _emit(event: string, payload: Uint8Array, participant: { identity: string } | null, topic: string) {
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
