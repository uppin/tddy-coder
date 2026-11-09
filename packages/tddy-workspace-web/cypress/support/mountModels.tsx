/**
 * Mount helper for the Models & Agents specs: the screen behind the three ports, with every host
 * answering from an in-memory backend.
 *
 * Replaces tddy-web's `mountWithRpc` + `withSelectedDaemon` + `perDaemonLiveKitRpc` trio. What those
 * expressed through a LiveKit room is stated here directly:
 *  - the directory listing hosts            -> `daemons` (+ `directoryStatus`);
 *  - a room that is not joined              -> `reachable: false` (no host can be dialled);
 *  - a daemon that left the room            -> `hostStatus: "idle"` (still dialable, no longer serving);
 *  - a different backend per daemon         -> `backendsByHost`.
 */
import type { ReactElement } from "react";
import { createClient, type Transport } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionProviderRegistry, type DaemonHost } from "tddy-client-web";
import type { ConnectionStatus, HostConnection } from "tddy-client-web/connections/types";
import { aClientsPortDouble, anAuthDouble } from "tddy-client-web/testkit";

export interface MountModelsOptions {
  /** The hosts the directory lists; the first is selected. */
  daemons: DaemonHost[];
  /** Each host's own backend, keyed by instance id; a host without an entry is served by `backend`. */
  backendsByHost?: Record<string, InMemoryRpcBackend>;
  /** `false`: no registered provider can reach any host (the common room is not joined). Default `true`. */
  reachable?: boolean;
  /** The directory's state. Default `connected`; `idle` is a room that never connected. */
  directoryStatus?: ConnectionStatus;
  /** The status every host connection reports. Default `connected`. */
  hostStatus?: ConnectionStatus;
  /** The signed-in operator's session token. Default `fake-token`. */
  sessionToken?: string;
}

function aHostServing(hostId: string, transport: Transport, status: ConnectionStatus): HostConnection {
  const clients = new Map<DescService, unknown>();
  return {
    hostId,
    providerId: "in-memory",
    status,
    error: null,
    capabilities: new Set(["rpc"]),
    clientFor: <S extends DescService>(service: S) => {
      if (!clients.has(service)) clients.set(service, createClient(service, transport));
      return clients.get(service) as ReturnType<typeof createClient<S>>;
    },
    transport: () => transport,
    openSession: () => {
      throw new Error("the models host double serves RPC only; it opens no session attachments");
    },
  };
}

/** `jsx` inside the clients, navigation and auth ports, hosts answering from `backend` (or their own). */
export function mountModels(jsx: ReactElement, backend: InMemoryRpcBackend, options: MountModelsOptions): Cypress.Chainable {
  const { daemons, backendsByHost = {}, reachable = true, directoryStatus = "connected", hostStatus = "connected", sessionToken = "fake-token" } = options;
  const base = aClientsPortDouble(backend, { daemons });
  const connections = new ConnectionProviderRegistry();
  const byHost = new Map<string, HostConnection>();
  connections.register({
    id: "in-memory",
    connectHost: (hostId) => {
      if (!reachable || !daemons.some((d) => d.instanceId === hostId)) return null;
      if (!byHost.has(hostId)) {
        byHost.set(hostId, aHostServing(hostId, (backendsByHost[hostId] ?? backend).transport(), hostStatus));
      }
      return byHost.get(hostId) ?? null;
    },
  });
  const clients = { ...base, connections, selectedDaemon: { ...base.selectedDaemon, directoryStatus } };
  return cy.mountWithClients(jsx, backend, { clients, auth: anAuthDouble({ sessionToken }) });
}
