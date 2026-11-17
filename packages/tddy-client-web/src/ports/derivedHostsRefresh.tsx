/**
 * The derived-hosts refresh port: how a screen that knows a listing went stale says so.
 *
 * Two moments make the hosts a daemon reaches change without the daemon set changing: a Coder
 * account is connected (its workspaces become listable) and the vault is unlocked (every account's
 * workspaces do). The Accounts screen and the vault prompt call {@link useRefreshDerivedHosts} at
 * those moments; the app's derived directory source provides the function and asks every connected
 * parent again. A page without the provider — a component test, a build with no directory — gets a
 * no-op, so calling it is always safe.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-16-remote-shell-derived-host-identity.md
 */

import { createContext, useContext, type ReactNode } from "react";
import { DERIVED_SOURCE_ID, type DerivedHostDirectorySource, type DerivedHostListingFailure } from "../hostDirectory/derivedSource";
import { useHostDirectorySource } from "../hostDirectory/useHostDirectory";

const NO_REFRESH = (): void => {};

const DerivedHostsRefreshContext = createContext<() => void>(NO_REFRESH);

export interface DerivedHostsRefreshProviderProps {
  /** Asks every connected parent for its derived hosts again. */
  readonly refresh: () => void;
  readonly children: ReactNode;
}

/** Provide the refresh to `children`. Mounted by the app beside its derived directory source. */
export function DerivedHostsRefreshProvider({ refresh, children }: DerivedHostsRefreshProviderProps) {
  return <DerivedHostsRefreshContext.Provider value={refresh}>{children}</DerivedHostsRefreshContext.Provider>;
}

/** The refresh in scope, or a no-op when no provider is mounted. */
export function useRefreshDerivedHosts(): () => void {
  return useContext(DerivedHostsRefreshContext);
}

/**
 * The parents whose derived-host listing failed, from the directory's derived source — what the
 * New-session form shows beside its Host field. Empty when there is no derived source.
 */
export function useDerivedHostListingFailures(): readonly DerivedHostListingFailure[] {
  const source = useHostDirectorySource(DERIVED_SOURCE_ID) as DerivedHostDirectorySource | undefined;
  return source?.failures ?? [];
}
