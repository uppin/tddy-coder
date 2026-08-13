/**
 * Common-room `Room` doubles for the **connection lifecycle** — the part `useCommonRoom` awaits.
 *
 * Companion to `fakeCommonRoom.ts`, which models a room that is *already* connected and emits
 * participant events. These model the step before that: joining the room, which can fail.
 *
 * Reproduces a production incident (2026-08-13): the browser's signal WebSocket to LiveKit
 * connected and then dropped ~14s later because ICE never established across a router boundary, so
 * `Room.connect()` rejected. `useCommonRoom` caught it correctly, but nothing downstream carried
 * the reason, so the UI sat on "Connecting to presence room…" indefinitely and the daemon selector
 * stayed silently empty.
 */

import type { Room } from "livekit-client";

/**
 * A `Room` that is wired for `useCommonRoom`'s usage — event subscription, `connect`, `disconnect`
 * and an empty roster — parameterised by how its `connect()` settles.
 */
function aRoomWhoseConnect(connect: () => Promise<void>): Room {
  return {
    localParticipant: null,
    remoteParticipants: new Map(),
    connect,
    disconnect: () => {},
    on: () => {},
    off: () => {},
  } as unknown as Room;
}

/**
 * A room that cannot be joined: `connect()` rejects with `reason`. This is what livekit-client
 * produces when the signal socket is reachable but the peer connection never establishes (blocked
 * ICE/UDP), and when the LiveKit URL is unreachable outright.
 */
export function aCommonRoomThatFailsToConnect(reason: string): Room {
  return aRoomWhoseConnect(() => Promise.reject(new Error(reason)));
}

/** A room whose `connect()` never settles — the genuinely still-joining case. */
export function aCommonRoomThatNeverFinishesConnecting(): Room {
  return aRoomWhoseConnect(() => new Promise<void>(() => {}));
}
