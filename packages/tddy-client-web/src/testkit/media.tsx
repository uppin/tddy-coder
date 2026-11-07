/** Doubles for the presence, room-RPC and media ports: rooms are plain tokens, participants a mutable list. */
import { createElement, useEffect, useMemo, useSyncExternalStore } from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { MediaPort, PresencePort, RoomHandle, RoomParticipant, RoomRpcPort, RoomState, RoomStatus } from "../ports/media";

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

/** An input event the double's `useScreenSharingInput` saw on the picture, in the order it was seen. */
export interface ForwardedInput {
  readonly kind: "pointer" | "key";
  readonly detail: unknown;
}

export interface MediaDouble extends MediaPort {
  /** Input forwarded by `useScreenSharingInput`, in order. */
  readonly forwardedInput: ReadonlyArray<ForwardedInput>;
  /** Give (or take) `identity` camera video in `room`; `useHasCameraVideo` readers re-render. */
  setCameraVideo(room: RoomHandle, identity: string, hasVideo: boolean): void;
  /** The presenter room a hint naming `roomName` joins, as `usePresenterRoom` reports it. */
  presentRoom(roomName: string, state: { room: RoomHandle | null; status: RoomStatus; error?: string | null }): void;
}

const roomName = (room: RoomHandle | null) => (room === null ? "" : (room as unknown as { name: string }).name);

/**
 * A media port whose state the test sets: camera video per participant, the presenter room per
 * room name. `useScreenSharingInput` listens for real pointer and keyboard events on the picture and
 * records what it would have forwarded, so a spec asserts on what the operator's input became and
 * not on a handler having run.
 */
export function aMediaDouble(overrides: Partial<MediaPort> = {}): MediaDouble {
  const store = aStore();
  const cameras = new Set<string>();
  const presenterRooms = new Map<string, RoomState>();
  const forwarded: ForwardedInput[] = [];
  const NO_ROOM: RoomState = { room: null, status: "idle", error: null };
  const cameraKey = (room: RoomHandle | null, identity: string) => `${roomName(room)}\u0000${identity}`;

  const double: MediaDouble = {
    get forwardedInput() {
      return forwarded;
    },
    setCameraVideo: (room, identity, hasVideo) => {
      if (hasVideo) cameras.add(cameraKey(room, identity));
      else cameras.delete(cameraKey(room, identity));
      store.notify();
    },
    presentRoom: (name, state) => {
      presenterRooms.set(name, { room: state.room, status: state.status, error: state.error ?? null });
      store.notify();
    },
    usePresenterRoom: (hint) => {
      useSyncExternalStore(store.subscribe, store.version);
      return (hint?.room !== undefined ? presenterRooms.get(hint.room) : undefined) ?? NO_ROOM;
    },
    useScreenSharingInput: ({ picture, onCloseChord }) => {
      useEffect(() => {
        const element = picture.current;
        if (!element) return;
        const pointer = (e: Event) => forwarded.push({ kind: "pointer", detail: e.type });
        const key = (e: KeyboardEvent) => {
          if (e.key === "Escape" && e.ctrlKey && e.altKey) {
            if (e.type === "keydown") onCloseChord();
            return;
          }
          forwarded.push({ kind: "key", detail: e.key });
        };
        const pointerEvents = ["pointerdown", "pointermove", "pointerup"] as const;
        for (const type of pointerEvents) element.addEventListener(type, pointer);
        document.addEventListener("keydown", key);
        document.addEventListener("keyup", key);
        return () => {
          for (const type of pointerEvents) element.removeEventListener(type, pointer);
          document.removeEventListener("keydown", key);
          document.removeEventListener("keyup", key);
        };
      }, [picture, onCloseChord]);
      return { inputUnavailable: false };
    },
    useHasCameraVideo: (room, identity) => {
      useSyncExternalStore(store.subscribe, store.version);
      return cameras.has(cameraKey(room, identity));
    },
    RemoteVideo: ({ identity, trackName, className, testId, videoRef }) =>
      createElement("video", { "data-testid": testId ?? `remote-video-${identity}-${trackName}`, className, ref: videoRef }),
    VideoPreview: ({ identity, open }) =>
      open ? createElement("div", { "data-testid": `video-preview-${identity}` }) : null,
  };
  return Object.assign(double, overrides);
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
