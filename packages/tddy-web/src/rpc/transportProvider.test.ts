/**
 * Unit tests for the production LiveKit transport's fault reporting.
 *
 * An inbound frame the transport cannot turn into a response is how a data-channel or reassembly
 * fault reaches the app — it surfaced as `[SessionManager] refresh error RangeError: premature EOF`,
 * with nothing naming the connection it came from. The production factory therefore gives the
 * transport a reporter that identifies the room and the target daemon.
 */

import { describe, it, expect, spyOn } from "bun:test";
import { RoomEvent } from "livekit-client";
import { createDefaultLiveKitTransport } from "./transportProvider";

type DataReceivedListener = (
  payload: Uint8Array,
  participant?: { identity?: string } | null,
  _kind?: unknown,
  topic?: string,
) => void;

/** A room stand-in that can deliver a `DataReceived` frame to whatever listens on it. */
function aFakeRoom(name: string) {
  const listeners: Record<string, DataReceivedListener[]> = {};
  const room = {
    name,
    on(event: string, handler: DataReceivedListener) {
      (listeners[event] ??= []).push(handler);
      return room;
    },
    off(_event: string, _handler: DataReceivedListener) {
      return room;
    },
    localParticipant: {
      identity: "web-uppin",
      publishData(_payload: Uint8Array, _opts: unknown) {},
    },
    _emit(event: string, payload: Uint8Array, participant: { identity: string }, topic: string) {
      for (const handler of (listeners[event] ?? []).slice()) handler(payload, participant, undefined, topic);
    },
  };
  return room;
}

/** The `[tddy][rpc]` lines the console received while `act` ran, in order. Read before the spy is
 *  restored, because `mockRestore()` also discards the recorded calls. */
function messagesReportedDuring(act: () => void): string[] {
  const consoleError = spyOn(console, "error").mockImplementation(() => {});
  act();
  const messages = consoleError.mock.calls.map((call: unknown[]) => String(call[0]));
  consoleError.mockRestore();
  return messages;
}

/** Bytes that are not a decodable `RpcResponse`: field 2 declares five bytes of `response_message`
 *  but only one follows — the wire shape of a truncated or mixed-up payload. */
function anUndecodableEnvelope(): Uint8Array {
  return new Uint8Array([0x12, 0x05, 0x68]);
}

/** A frame flagged as chunked (leading `0x00`, the chunk magic) but shorter than the 13-byte chunk
 *  header the codec requires. */
function aTruncatedChunkFrame(): Uint8Array {
  return new Uint8Array([0x00, 0x01, 0x02]);
}

describe("production LiveKit transport — reporting undecodable inbound frames", () => {
  it("names the room and target daemon when a payload does not decode as a response", () => {
    // Given — the production transport for the common room, targeting one daemon
    const room = aFakeRoom("tddy-lobby");
    createDefaultLiveKitTransport(room as any, "daemon-udoo");

    // When — that daemon delivers bytes that do not decode as an `RpcResponse`
    const reported = messagesReportedDuring(() =>
      room._emit(RoomEvent.DataReceived, anUndecodableEnvelope(), { identity: "daemon-udoo" }, "tddy-rpc"),
    );

    // Then — the fault is reported against the connection it arrived on
    expect(reported).toEqual([
      "[tddy][rpc] room=tddy-lobby target=daemon-udoo: decode error from sender=daemon-udoo",
    ]);
  });

  it("names the room and target daemon when a chunk frame is malformed", () => {
    // Given — the production transport for the common room, targeting one daemon
    const room = aFakeRoom("tddy-lobby");
    createDefaultLiveKitTransport(room as any, "daemon-udoo");

    // When — that daemon delivers a chunk frame too short to carry a chunk header
    const reported = messagesReportedDuring(() =>
      room._emit(RoomEvent.DataReceived, aTruncatedChunkFrame(), { identity: "daemon-udoo" }, "tddy-rpc"),
    );

    // Then — the fault is reported against the connection it arrived on
    expect(reported).toEqual([
      "[tddy][rpc] room=tddy-lobby target=daemon-udoo: malformed chunk frame from sender=daemon-udoo",
    ]);
  });
});
