/**
 * The derived host-directory source for this app: every connected daemon is asked for the hosts
 * it reaches through a remote shell, and its answers join the directory under the `derived` id.
 *
 * It lives inside `ConnectionProviders` because a parent is asked over its own `HostConnection` —
 * there is no wire to a daemon above that provider. The merged directory it publishes shadows the
 * base sources for the subtree below it, which is where every host-selection surface reads.
 */

import { useEffect, useState, type ReactNode } from "react";
import {
  HostDirectorySources,
  NO_DERIVED_HOSTS,
  listDerivedHosts,
  useAuth,
  useHostConnector,
  useHostDirectory,
  type DerivedHostDirectorySource,
  type DerivedHostParent,
} from "tddy-client-web";
import { HostService } from "tddy-client-web/gen/host_pb";

/**
 * Provide the base directory sources plus the derived source to `children`.
 *
 * Mount inside `ConnectionProviders` (a clients port with a registry must be in scope).
 */
export function DerivedHostDirectorySources({ children }: { children: ReactNode }) {
  const directory = useHostDirectory();
  const connect = useHostConnector();
  const { sessionToken } = useAuth();
  const [derived, setDerived] = useState<DerivedHostDirectorySource>(NO_DERIVED_HOSTS);

  // The parents are the directory's daemons — never a derived host, which is asked through its
  // parent. Keyed on their ids so a rebuilt list with the same daemons asks nothing again.
  const parentIdsKey = directory.hosts
    .filter((host) => !host.parent)
    .map((host) => host.hostId)
    .join("\n");

  useEffect(() => {
    const parents: DerivedHostParent[] = directory.hosts
      .filter((host) => !host.parent)
      .map((host) => {
        const connection = connect(host.hostId);
        return {
          host,
          status: connection?.status ?? "error",
          // Only a connected parent is asked, so a missing connection is never called.
          client: connection?.clientFor(HostService) as DerivedHostParent["client"],
        };
      })
      .filter((parent) => parent.client !== undefined && parent.client !== null);
    let current = true;
    void listDerivedHosts(parents, sessionToken ?? "").then((source) => {
      if (current) setDerived(source);
    });
    return () => {
      current = false;
    };
    // `directory.hosts` is rebuilt on every source change; the ids are the stable input.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parentIdsKey, connect, sessionToken]);

  return (
    <HostDirectorySources sources={[...directory.sources, derived]}>{children}</HostDirectorySources>
  );
}
