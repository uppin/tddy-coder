import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  MediaPortProvider,
  PresencePortProvider,
  type ConnectionProviderRegistry,
  type DaemonHost,
  type MediaPort,
  type PresencePort,
} from "tddy-client-web";
import {
  aClientsPortDouble,
  aMediaDouble,
  aPresenceDouble,
  withClients,
  type MountWithClientsOverrides,
} from "tddy-client-web/testkit";
import { aHostConnection, aRegistryServing } from "./rpc/hostConnections";

/** What a hosts spec may state about the world it mounts into. */
export interface HostsMountOverrides extends MountWithClientsOverrides {
  /** The hosts the directory lists; the first is selected. Default: one host, `host-a`. */
  daemons?: DaemonHost[];
  /** Resolve hosts through this registry instead of the default: every listed host, reached over LiveKit (it carries RPC, media and presence, as the shared common room did for every host). */
  registry?: ConnectionProviderRegistry;
  /** The instance that serves the page, when the spec cares which host is selected. */
  selectedInstanceId?: string | null;
  media?: MediaPort;
  presence?: PresencePort;
}

/** The three client ports (backed by `backend`) plus the presence and media doubles every host surface reads. */
export function withHostsPorts(jsx: ReactElement, backend: InMemoryRpcBackend, overrides: HostsMountOverrides = {}): ReactElement {
  const { daemons, registry, selectedInstanceId, media, presence, ...ports } = overrides;
  const double = aClientsPortDouble(backend, { daemons, selectedInstanceId });
  const reachable = registry ?? aRegistryServing(
    ...double.selectedDaemon.daemons.map((d) => aHostConnection(d.instanceId).reachedOverLiveKit().servingOver(backend.transport()).build()),
  );
  const clients = ports.clients ?? { ...double, connections: reachable };
  return (
    <PresencePortProvider port={presence ?? aPresenceDouble()}>
      <MediaPortProvider port={media ?? aMediaDouble()}>{withClients(jsx, backend, { ...ports, clients })}</MediaPortProvider>
    </PresencePortProvider>
  );
}
