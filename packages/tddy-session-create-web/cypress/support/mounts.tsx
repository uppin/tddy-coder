/**
 * Mounts for specs whose hosts answer from backends of their own.
 *
 * `cy.mountWithClients(jsx, backend, { clients: aClientsPortDouble(backend, { daemons }) })` serves
 * every host from one backend. A spec that proves a fan-out reached host B — an option appearing under
 * B is proof only if B answered from B's own handlers — needs each host on its own transport, and a
 * record of which hosts the form asked for.
 */

import type { ReactElement } from "react";
import type { DescService } from "@bufbuild/protobuf";
import { createClient, type Transport } from "@connectrpc/connect";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionProviderRegistry, type ClientsPort, type DaemonHost, type HostConnection } from "tddy-client-web";
import { aClientsPortDouble } from "tddy-client-web/testkit";

/** A host that answers every call from `transport`. */
function aHostOver(hostId: string, transport: Transport): HostConnection {
  const clients = new Map<DescService, unknown>();
  return {
    hostId,
    providerId: "in-memory",
    status: "connected",
    error: null,
    capabilities: new Set(["rpc"]),
    clientFor: <S extends DescService>(service: S) => {
      if (!clients.has(service)) clients.set(service, createClient(service, transport));
      return clients.get(service) as ReturnType<typeof createClient<S>>;
    },
    transport: () => transport,
    openSession: () => {
      throw new Error("the in-memory host double serves RPC only; it opens no session attachments");
    },
  };
}

export interface PerHostClients {
  port: ClientsPort;
  /** Every host id a connection was asked for, in order. */
  targets: string[];
}

/** A clients port whose host `id` answers from `backendsByHost[id]`; HTTP answers from `httpBackend`. */
export function aClientsPortOverHosts(
  backendsByHost: Record<string, InMemoryRpcBackend>,
  options: { httpBackend: InMemoryRpcBackend; daemons: DaemonHost[] },
): PerHostClients {
  const base = aClientsPortDouble(options.httpBackend, { daemons: options.daemons });
  const targets: string[] = [];
  const connections = new ConnectionProviderRegistry();
  const byHost = new Map<string, HostConnection>();
  connections.register({
    id: "in-memory",
    connectHost: (hostId) => {
      targets.push(hostId);
      const backend = backendsByHost[hostId];
      if (!backend) return null;
      if (!byHost.has(hostId)) byHost.set(hostId, aHostOver(hostId, backend.transport()));
      return byHost.get(hostId) ?? null;
    },
  });
  return { port: { ...base, connections }, targets };
}

/** Mount `jsx` with each host answering from its own backend. */
export function mountWithHostBackends(
  jsx: ReactElement,
  backendsByHost: Record<string, InMemoryRpcBackend>,
  options: { httpBackend: InMemoryRpcBackend; daemons: DaemonHost[] },
): { targets: string[] } {
  const { port, targets } = aClientsPortOverHosts(backendsByHost, options);
  cy.mountWithClients(jsx, options.httpBackend, { clients: port });
  return { targets };
}
