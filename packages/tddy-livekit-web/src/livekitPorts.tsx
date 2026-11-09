/**
 * The one registration call `tddy-web` makes for LiveKit.
 *
 * `livekitPorts()` builds the LiveKit implementations of the presence, room-RPC and media ports once, and
 * {@link LiveKitPortsProvider} mounts them. Hooks on a port are called like any imported hook, so
 * their identities are fixed for the app's lifetime — which is why the app builds the ports once.
 */
import { useCallback, useEffect, useMemo, useState, type ReactElement, type ReactNode } from "react";
import { createClient, type Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import { ConnectionState, RoomEvent, type Room } from "livekit-client";
import {
  MediaPortProvider,
  PresencePortProvider,
  RoomRpcPortProvider,
  type MediaPort,
  type PresencePort,
  type RoomHandle,
  type RoomRpcPort,
  type RoomStatus,
} from "tddy-client-web";
import { asRoom } from "./roomHandle";
import { livekitMedia } from "./media/livekitMedia";
import { useHostPresence } from "./presence/useHostPresence";
import { usePresenceRoom } from "./presence/presenceRoom";
import { useRoomParticipants } from "./rooms/useRoomParticipants";
import { useLiveKitTransportFactory, useLiveKitTransportFactoryIsOverridden } from "./transport/liveKitTransport";

export interface LiveKitPorts {
  readonly presence: PresencePort;
  readonly roomRpc: RoomRpcPort;
  readonly media: MediaPort;
}

/** Whether `identity` is in `room` right now, per the SDK's own roster. A fake room with no roster is nobody's host. */
function isRemoteHere(room: Room, identity: string): boolean {
  return room.remoteParticipants?.has(identity) ?? false;
}

/** Joined, or still on the way there: a never-connected SDK room reports `Disconnected`, as `useCommonRoom` reads it too. */
function statusOf(state: ConnectionState): RoomStatus {
  return state === ConnectionState.Connected ? "connected" : "connecting";
}

const roomRpc: RoomRpcPort = {
  useRoomClient<S extends DescService>(handle: RoomHandle | null, identity: string | null, service: S): Client<S> | null {
    const factory = useLiveKitTransportFactory();
    const overridden = useLiveKitTransportFactoryIsOverridden();
    const room = asRoom(handle);
    // The production default needs a genuinely connected `Room` and crashes without one; a test
    // double (an overridden factory) is free to ignore `room` entirely, e.g. to route to an
    // in-memory RPC backend. So a client is built without a room only for a double.
    const canBuild = identity !== null && (room !== null || overridden);
    return useMemo(
      () => (canBuild ? createClient(service, factory(handle as RoomHandle, identity)) : null),
      // `identity` is non-null whenever `canBuild`; the room is read by identity of the handle.
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [factory, canBuild, room, identity, service],
    );
  },

  useParticipantPresent(handle, identity) {
    const room = asRoom(handle);
    const read = () => (room !== null && identity !== null ? isRemoteHere(room, identity) : false);
    const [present, setPresent] = useState(read);
    useEffect(() => {
      setPresent(read());
      if (room === null || typeof room.on !== "function") return;
      const sync = () => setPresent(read());
      room.on(RoomEvent.ParticipantConnected, sync);
      room.on(RoomEvent.ParticipantDisconnected, sync);
      room.on(RoomEvent.Reconnected, sync);
      return () => {
        room.off(RoomEvent.ParticipantConnected, sync);
        room.off(RoomEvent.ParticipantDisconnected, sync);
        room.off(RoomEvent.Reconnected, sync);
      };
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [room, identity]);
    return present;
  },

  useParticipantProbe(handle, identity) {
    const room = asRoom(handle);
    return useCallback(() => (room !== null && identity !== null ? isRemoteHere(room, identity) : false), [room, identity]);
  },

  useRoomStatus(handle) {
    const room = asRoom(handle);
    const [status, setStatus] = useState<RoomStatus>(() => (room ? statusOf(room.state) : "idle"));
    useEffect(() => {
      setStatus(room ? statusOf(room.state) : "idle");
      if (room === null || typeof room.on !== "function") return;
      const sync = (state: ConnectionState) => setStatus(statusOf(state));
      room.on(RoomEvent.ConnectionStateChanged, sync);
      return () => {
        room.off(RoomEvent.ConnectionStateChanged, sync);
      };
    }, [room]);
    return status;
  },
};

const presence: PresencePort = {
  useHostPresence,
  usePresenceRoom,
  useRoomParticipants: (handle) => useRoomParticipants(asRoom(handle)),
};

/** Build the LiveKit implementations of the presence, room-RPC and media ports. */
export function livekitPorts(): LiveKitPorts {
  return { presence, roomRpc, media: livekitMedia };
}

/** Mount the ports {@link livekitPorts} built. */
export function LiveKitPortsProvider({ ports, children }: { ports: LiveKitPorts; children?: ReactNode }): ReactElement {
  return (
    <PresencePortProvider port={ports.presence}>
      <RoomRpcPortProvider port={ports.roomRpc}>
        <MediaPortProvider port={ports.media}>{children}</MediaPortProvider>
      </RoomRpcPortProvider>
    </PresencePortProvider>
  );
}
