/**
 * A `RoomHandle` is what the rest of the app holds: opaque, never a LiveKit type. Inside this package
 * it is a `livekit-client` `Room`, and these two functions are the only places that says so.
 */
import type { Room } from "livekit-client";
import type { RoomHandle } from "tddy-client-web";

export function asRoom(handle: RoomHandle | null | undefined): Room | null {
  return (handle ?? null) as unknown as Room | null;
}

export function asHandle(room: Room | null | undefined): RoomHandle | null {
  return (room ?? null) as unknown as RoomHandle | null;
}
