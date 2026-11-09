/**
 * Room stand-ins for specs that mount a provider over a common room and care about *what is in it*,
 * not about LiveKit. They return opaque handles, so a spec holding one imports nothing from
 * `livekit-client`.
 */
import { ConnectionState, Room } from "livekit-client";
import type { RoomHandle } from "tddy-client-web";

/** A freshly constructed SDK room: never connected, reports `ConnectionState.Disconnected` (so a "connecting" common room). */
export function aDisconnectedRoom(): RoomHandle {
  return new Room() as unknown as RoomHandle;
}

/**
 * A stand-in for a common room this page has **joined**, carrying the given remote participants,
 * each with its own `metadata` (a JSON document such as a `session` block, for presence-driven
 * session metadata rendering). `aDisconnectedRoom()` is not that: a fresh SDK room reports
 * `Disconnected`, which `useCommonRoom` maps to `"connecting"` — "still joining", not "joined".
 */
export function aJoinedRoom(participants: ReadonlyArray<string | { identity: string; metadata: string }> = []): RoomHandle {
  const remoteParticipants = new Map(
    participants.map((p) => {
      const { identity, metadata } = typeof p === "string" ? { identity: p, metadata: "" } : p;
      return [identity, { identity, metadata, joinedAt: new Date() }] as const;
    }),
  );
  return {
    localParticipant: undefined,
    remoteParticipants,
    state: ConnectionState.Connected,
    on: () => {},
    off: () => {},
  } as unknown as RoomHandle;
}

/** The SDK's connection states, for fakes that report one (`state`). */
export { ConnectionState as RoomConnectionState } from "livekit-client";
