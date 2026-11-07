/** Doubles for the presence, room-RPC and media ports: rooms are plain tokens, participants a mutable list. */
import { createElement, useMemo, useSyncExternalStore } from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { MediaPort, PresencePort, RoomHandle, RoomParticipant, RoomRpcPort } from "../ports/media";

/** A subscribable cell: what each double's readers re-render on. */
function aStore() {
  const listeners = new Set<() => void>();
  let version = 0;
  return {
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    version: () => version,
    notify: () => {
      version += 1;
      for (const listener of [...listeners]) listener();
    },
  };
}

function aRoomRegistry() {
  const rooms = new Map<string, RoomHandle>();
  return (name: string): RoomHandle => {
    if (!rooms.has(name)) rooms.set(name, { name } as unknown as RoomHandle);
    return rooms.get(name) as RoomHandle;
  };
}

export interface PresenceDouble extends PresencePort {
  /** A room handle for `name`, stable per name. */
  room(name: string): RoomHandle;
  /** Make `hostId`'s presence room `name`. */
  presentHost(hostId: string, name: string): void;
  /** Replace `room`'s participants and notify subscribers. */
  setParticipants(room: RoomHandle, participants: RoomParticipant[]): void;
}

const NO_PARTICIPANTS: RoomParticipant[] = [];

export function aPresenceDouble(): PresenceDouble {
  const room = aRoomRegistry();
  const store = aStore();
  const hosts = new Map<string, RoomHandle>();
  const participants = new Map<RoomHandle, RoomParticipant[]>();
  let presenceRoom: RoomHandle | null = null;
  return {
    room,
    presentHost: (hostId, name) => {
      const handle = room(name);
      hosts.set(hostId, handle);
      presenceRoom = handle;
      store.notify();
    },
    setParticipants: (handle, list) => {
      participants.set(handle, list);
      store.notify();
    },
    useHostPresence: (hostId) => {
      useSyncExternalStore(store.subscribe, store.version);
      return hostId === null ? null : (hosts.get(hostId) ?? null);
    },
    usePresenceRoom: () => {
      useSyncExternalStore(store.subscribe, store.version);
      return presenceRoom;
    },
    useRoomParticipants: (handle) => {
      useSyncExternalStore(store.subscribe, store.version);
      return handle === null ? NO_PARTICIPANTS : (participants.get(handle) ?? NO_PARTICIPANTS);
    },
  };
}

export interface MediaDouble extends MediaPort {
  /** Input forwarded by `useScreenSharingInput`, in order. */
  readonly forwardedInput: ReadonlyArray<{ kind: "pointer" | "key"; detail: unknown }>;
  /** Give (or take) `identity` camera video in `room`; `useHasCameraVideo` readers re-render. */
  setCameraVideo(room: RoomHandle, identity: string, hasVideo: boolean): void;
  /** The presenter room a hint naming `roomName` joins, as `usePresenterRoom` reports it. */
  presentRoom(roomName: string, state: { room: RoomHandle | null; status: "idle" | "connecting" | "connected" | "error"; error?: string | null }): void;
}

// TODO(media): #855 settles the final MediaPort shape and what its double records; this is the
// minimal working double the plumbing needs until then.
export function aMediaDouble(overrides: Partial<MediaPort> = {}): MediaDouble {
  return {
    forwardedInput: [],
    usePresenterRoom: () => ({ room: null, status: "idle", error: null }),
    useScreenSharingInput: () => ({ inputUnavailable: false }),
    useHasCameraVideo: () => false,
    RemoteVideo: ({ identity, trackName }) => createElement("div", { "data-testid": `remote-video-${identity}-${trackName}` }),
    setCameraVideo: () => {
      throw new Error("TODO(media): implement");
    },
    presentRoom: () => {
      throw new Error("TODO(media): implement");
    },
    ...overrides,
  };
}

export interface RoomRpcDouble extends RoomRpcPort {
  /** Put `identity` in or out of `room`, re-rendering `useParticipantPresent` readers. */
  setPresent(room: RoomHandle, identity: string, present: boolean): void;
}

/** A room-RPC port whose every client is `backend`'s; participants present only when set. */
export function aRoomRpcDouble(backend: InMemoryRpcBackend): RoomRpcDouble {
  const store = aStore();
  const present = new Set<string>();
  const key = (room: RoomHandle, identity: string) => `${(room as unknown as { name: string }).name}\u0000${identity}`;
  const transport = backend.transport();
  return {
    setPresent: (room, identity, here) => {
      if (here) present.add(key(room, identity));
      else present.delete(key(room, identity));
      store.notify();
    },
    useRoomClient: <S extends DescService>(room: RoomHandle | null, identity: string | null, service: S): Client<S> | null =>
      // eslint-disable-next-line react-hooks/rules-of-hooks
      useMemo(() => (room !== null && identity !== null ? createClient(service, transport) : null), [room, identity, service]),
    useParticipantPresent: (room, identity) => {
      useSyncExternalStore(store.subscribe, store.version);
      return room !== null && identity !== null && present.has(key(room, identity));
    },
    useRoomStatus: (room) => (room === null ? "idle" : "connected"),
  };
}
