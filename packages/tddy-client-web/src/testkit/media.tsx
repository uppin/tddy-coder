/** Doubles for the presence and media ports: rooms are plain tokens, participants a mutable list. */
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { MediaPort, PresencePort, RoomHandle, RoomParticipant, RoomRpcPort } from "../ports/media";

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

export interface RoomRpcDouble extends RoomRpcPort {
  /** Put `identity` in or out of `room`, re-rendering `useParticipantPresent` readers. */
  setPresent(room: RoomHandle, identity: string, present: boolean): void;
}

/** A room-RPC port whose every client is `backend`'s; participants present only when set. */
export function aRoomRpcDouble(backend: InMemoryRpcBackend): RoomRpcDouble {
  void backend;
  throw new Error("TODO(livekit): implement");
}
