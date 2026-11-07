/**
 * What a drawer spec mounts into: the ports the sessions drawer reads, backed by in-memory
 * backends, in place of the app's providers.
 *
 * - **Clients** — every host the tree asks the registry for answers over a transport the spec chose
 *   (one backend for all, or one per daemon), reached "over LiveKit" so the media and presence
 *   capabilities the drawer gates on are present. Calls are recorded per host as
 *   `daemon-<instanceId>`, the identity the LiveKit provider addresses a daemon by.
 * - **Presence / media / room-RPC** — the doubles from `tddy-client-web/testkit`.
 * - **Navigation** — the real URL hash, so deep links and Back behave as in the app.
 *
 * `withSelectedDaemon` (see `./withSelectedDaemon`) then states which daemons are listed and who is
 * in the common room, inside whichever mount this module built.
 */

import React, { type ReactElement } from "react";
import type { Transport } from "@connectrpc/connect";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  MediaPortProvider,
  PresencePortProvider,
  RoomRpcPortProvider,
  daemonRpcIdentity,
  type ClientsPort,
  type RoomHandle,
  type RoomRpcPort,
} from "tddy-client-web";
import { ConnectionProviderRegistry } from "tddy-client-web/connections/registry";
import { aClientsPortDouble, aMediaDouble, aPresenceDouble, aRoomRpcDouble } from "tddy-client-web/testkit";
import { aHostConnection } from "./hostConnections";
import { hashNavigationPort } from "../hashNavigation";

/** A recorded RPC: which daemon participant identity it was addressed to and the method called. */
export interface RecordedRpcCall {
  targetIdentity: string;
  /** The proto-declared method name (e.g. `SignalSession`). */
  method: string;
  stream: boolean;
}

export interface RoutedMount {
  /** Participant identities a host connection was built for, in build order. */
  targets: string[];
  /** Every RPC, in call order, with the identity it was addressed to. */
  rpcCalls: RecordedRpcCall[];
}

export interface Routing {
  /** Answers the serving daemon's own HTTP calls. */
  httpBackend: InMemoryRpcBackend;
  /** The transport host `hostId` answers over; defaults to the HTTP backend's. */
  transportFor?: (hostId: string) => Transport;
}

function recording(hostId: string, wire: Transport, recorded: RoutedMount): Transport {
  const targetIdentity = daemonRpcIdentity(hostId);
  return {
    unary: (method, signal, timeoutMs, header, input, contextValues) => {
      recorded.rpcCalls.push({ targetIdentity, method: method.name, stream: false });
      return wire.unary(method, signal, timeoutMs, header, input, contextValues);
    },
    stream: (method, signal, timeoutMs, header, input, contextValues) => {
      recorded.rpcCalls.push({ targetIdentity, method: method.name, stream: true });
      return wire.stream(method, signal, timeoutMs, header, input, contextValues);
    },
  };
}

/** A clients port whose registry resolves any host id to a LiveKit-reached host over the routed transport. */
function routedClientsPort(routing: Routing, recorded: RoutedMount): ClientsPort {
  const base = aClientsPortDouble(routing.httpBackend);
  const http = routing.httpBackend.transport();
  const connections = new ConnectionProviderRegistry();
  const byHost = new Map<string, ReturnType<ReturnType<typeof aHostConnection>["build"]>>();
  connections.register({
    id: "in-memory",
    connectHost: (hostId) => {
      if (!byHost.has(hostId)) {
        recorded.targets.push(daemonRpcIdentity(hostId));
        const wire = routing.transportFor?.(hostId) ?? http;
        byHost.set(hostId, aHostConnection(hostId).reachedOverLiveKit().servingOver(recording(hostId, wire, recorded)).build());
      }
      return byHost.get(hostId) ?? null;
    },
  });
  return { ...base, connections };
}

const NO_PRESENTER_ROOM = { room: null, status: "idle", error: null } as const;

/**
 * The doubles every drawer surface reads, around `children`.
 *
 * A session whose attachment hint names a room joins it as the presenter room: the media double
 * answers every such hint with a connected room, and the room-RPC double treats the session's own
 * process as present in it — what the old mount's room-agnostic LiveKit factory amounted to, and what
 * a chat or usage tab needs before it opens its stream.
 */
function DrawerPorts({ backend, children }: { backend: InMemoryRpcBackend; children?: React.ReactNode }): ReactElement {
  const [presence] = React.useState(() => aPresenceDouble());
  const [rooms] = React.useState(() => aRoomRpcDouble(backend));
  const [presenterRooms] = React.useState(() => new Set<RoomHandle>());
  const [media] = React.useState(() =>
    aMediaDouble({
      usePresenterRoom: (hint) => {
        if (!hint?.room) return NO_PRESENTER_ROOM;
        const room = presence.room(hint.room);
        presenterRooms.add(room);
        return { room, status: "connected", error: null };
      },
    }),
  );
  const [roomRpc] = React.useState<RoomRpcPort>(() => ({
    ...rooms,
    useParticipantPresent: (room, identity) => {
      const listed = rooms.useParticipantPresent(room, identity);
      return listed || (room !== null && presenterRooms.has(room));
    },
    useParticipantProbe: (room, identity) => {
      const listed = rooms.useParticipantProbe(room, identity);
      return React.useCallback(() => listed() || (room !== null && presenterRooms.has(room)), [listed, room]);
    },
  }));
  return (
    <PresencePortProvider port={presence}>
      <RoomRpcPortProvider port={roomRpc}>
        <MediaPortProvider port={media}>{children}</MediaPortProvider>
      </RoomRpcPortProvider>
    </PresencePortProvider>
  );
}

/** Mount `component` into the routed world. */
export function mountRouted(component: ReactElement, routing: Routing): RoutedMount {
  const recorded: RoutedMount = { targets: [], rpcCalls: [] };
  cy.mountWithClients(<DrawerPorts backend={routing.httpBackend}>{component}</DrawerPorts>, routing.httpBackend, {
    clients: routedClientsPort(routing, recorded),
    navigation: hashNavigationPort,
  });
  return recorded;
}
