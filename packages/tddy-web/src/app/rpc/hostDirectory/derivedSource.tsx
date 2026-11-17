/**
 * The derived host-directory source for this app: every connected daemon is asked for the hosts
 * it reaches through a remote shell, and its answers join the directory under the `derived` id.
 *
 * It lives inside `ConnectionProviders` because a parent is asked over its own `HostConnection` —
 * there is no wire to a daemon above that provider. The merged directory it publishes shadows the
 * base sources for the subtree below it, which is where every host-selection surface reads.
 */

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  DerivedHostsRefreshProvider,
  HostDirectorySources,
  NO_DERIVED_HOSTS,
  askParentForDerivedHosts,
  mergeParentAnswer,
  parentsToAsk,
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
  const askedRef = useRef<ReadonlySet<string>>(new Set());

  // The parents are the directory's daemons — never a derived host, which is asked through its
  // parent.
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

  const parentsRef = useRef(parents);
  parentsRef.current = parents;
  const tokenRef = useRef(sessionToken ?? "");
  tokenRef.current = sessionToken ?? "";

  const ask = useCallback((toAsk: readonly DerivedHostParent[]) => {
    for (const parent of toAsk) {
      askedRef.current = new Set(askedRef.current).add(parent.host.hostId);
      void askParentForDerivedHosts(parent, tokenRef.current).then((answer) => {
        setDerived((previous) => mergeParentAnswer(previous, answer));
      });
    }
  }, []);

  const refresh = useCallback(() => {
    ask(parentsToAsk(parentsRef.current, askedRef.current, true));
  }, [ask]);

  // Keyed on ids and statuses so a rebuilt list with the same daemons asks nothing again, and a
  // parent that becomes connected is asked.
  const parentsKey = parents.map((parent) => `${parent.host.hostId}:${parent.status}`).join("\n");

  useEffect(() => {
    ask(parentsToAsk(parentsRef.current, askedRef.current, false));
    // `parentsRef` carries the current parents; the key is the stable input.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [parentsKey, ask, sessionToken]);

  return (
    <DerivedHostsRefreshProvider refresh={refresh}>
      <HostDirectorySources sources={[...directory.sources, derived]}>{children}</HostDirectorySources>
    </DerivedHostsRefreshProvider>
  );
}
