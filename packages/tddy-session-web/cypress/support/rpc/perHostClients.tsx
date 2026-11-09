/**
 * Cypress adapter for cross-host specs where *different* daemons must answer with *different*
 * in-memory backends — the clients-port counterpart of the per-daemon LiveKit mount tddy-web's
 * specs use.
 *
 * The serving daemon is read over the HTTP transport the browser already holds (`httpBackend`);
 * every other host is a peer reached through the connection registry, and answers from its own
 * backend. A host the registry is asked for is recorded in `dialled`, which is how a spec says
 * "this host was addressed as a peer, and that one was not".
 *
 * Whether a peer is carried over LiveKit (identity `daemon-<hostId>`) is the LiveKit connection
 * provider's concern, and is pinned by `tddy-livekit-web`'s `connections/liveKit.test.ts`.
 */

import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import type { DaemonHost } from "tddy-client-web";
import { ConnectionProviderRegistry } from "tddy-client-web/connections/registry";
import { aClientsPortDouble } from "tddy-client-web/testkit";
import { aHostConnection } from "./hostConnections";

export interface PerHostClientsOptions {
  /** Answers the serving daemon's HTTP calls. */
  httpBackend: InMemoryRpcBackend;
  /** The hosts the directory lists; the first is selected unless `selectedInstanceId` says otherwise. */
  daemons: DaemonHost[];
  /** The daemon that served the web bundle. */
  servingInstanceId: string;
}

/** The host ids the mounted tree asked the registry to connect to. */
export type DialledHosts = string[];

export function mountWithPerHostClients(
  component: ReactElement,
  backendsByHost: Record<string, InMemoryRpcBackend>,
  options: PerHostClientsOptions,
): { dialled: DialledHosts } {
  const dialled: DialledHosts = [];
  const base = aClientsPortDouble(options.httpBackend, { daemons: options.daemons });
  const connections = new ConnectionProviderRegistry();
  const byHost = new Map<string, ReturnType<ReturnType<typeof aHostConnection>["build"]>>();
  connections.register({
    id: "in-memory",
    connectHost: (hostId) => {
      const backend = backendsByHost[hostId];
      if (!backend) return null;
      dialled.push(hostId);
      if (!byHost.has(hostId)) byHost.set(hostId, aHostConnection(hostId).servingOver(backend.transport()).build());
      return byHost.get(hostId) ?? null;
    },
  });
  const clients = {
    ...base,
    connections,
    selectedDaemon: { ...base.selectedDaemon, servingInstanceId: options.servingInstanceId },
  };
  cy.mountWithClients(component, options.httpBackend, { clients });
  return { dialled };
}
