/**
 * Mount into the drawer world with a different in-memory backend per daemon.
 *
 * Keys are the `daemon-<instanceId>` identities (`daemonRpcIdentity(instanceId)`); a host with no
 * entry answers from the HTTP backend, which also serves the serving daemon's own calls.
 */
import type { ReactElement } from "react";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { daemonRpcIdentity } from "tddy-client-web";
import { mountRouted } from "./drawerWorld";

export function mountWithPerDaemonLiveKitRpc(
  component: ReactElement,
  backendsByIdentity: Record<string, InMemoryRpcBackend>,
  options: { httpBackend: InMemoryRpcBackend },
): { targets: string[] } {
  const { targets } = mountRouted(component, {
    httpBackend: options.httpBackend,
    transportFor: (hostId) => (backendsByIdentity[daemonRpcIdentity(hostId)] ?? options.httpBackend).transport(),
  });
  return { targets };
}
