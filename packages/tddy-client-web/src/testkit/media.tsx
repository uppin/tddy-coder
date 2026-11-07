/** Doubles for the presence and media ports: rooms are plain tokens, participants a mutable list. */
import type { MediaPort, PresencePort, RoomHandle, RoomParticipant } from "../ports/media";

export interface PresenceDouble extends PresencePort {
  /** A room handle for `name`, stable per name. */
  room(name: string): RoomHandle;
  /** Make `hostId`'s presence room `name`. */
  presentHost(hostId: string, name: string): void;
  /** Replace `room`'s participants and notify subscribers. */
  setParticipants(room: RoomHandle, participants: RoomParticipant[]): void;
}

export function aPresenceDouble(): PresenceDouble {
  throw new Error("TODO(livekit): implement");
}

export interface MediaDouble extends MediaPort {
  /** Input forwarded by `useScreenSharingInput`, in order. */
  readonly forwardedInput: ReadonlyArray<{ kind: "pointer" | "key"; detail: unknown }>;
}

export function aMediaDouble(overrides: Partial<MediaPort> = {}): MediaDouble {
  void overrides;
  throw new Error("TODO(livekit): implement");
}
