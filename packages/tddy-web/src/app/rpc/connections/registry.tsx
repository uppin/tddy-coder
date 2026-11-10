/**
 * The app's side of the connection registry: the one registry for the page's lifetime, the wires
 * that register into it, and the readiness predicate the directory publishes.
 *
 * The registry class and the hooks that resolve a host through it live in `tddy-client-web`; what is
 * here is what `tddy-web` supplies as `ClientsPort.connections` and `ClientsPort.isHostReady`.
 * `AppClientsPorts` mounts them below every provider in this file.
 */

import { createContext, useContext, useRef, type ReactNode } from "react";
import { ConnectionProviderRegistry } from "tddy-client-web";
import { AppClientsPorts } from "../appClientsPorts";

const ConnectionProviderContext = createContext<ConnectionProviderRegistry | null>(null);

/**
 * Whether a host has been announced yet — `null` when nothing in scope restricts it.
 *
 * Reaching a host is a different question from being *ready* to be asked: a host the page was
 * pointed at (a reload onto `?host=laptop-b`) can be resolved by a wire long before the directory
 * has listed it, and a request sent to it in that window has nobody to answer it. The directory is
 * what knows, so whoever owns it provides the answer here and the hooks below withhold every
 * connection to a host that is not ready. Without a provider nothing is withheld, which keeps a
 * subtree that has no directory (the playground, a bare registry in a unit test) working.
 */
export const HostReadinessContext = createContext<((hostId: string) => boolean) | null>(null);

/** Provide the readiness predicate for hosts resolved below. Identity changes when readiness does. */
export function HostReadiness({
  isReady,
  children,
}: {
  isReady: (hostId: string) => boolean;
  children: ReactNode;
}) {
  return (
    <HostReadinessContext.Provider value={isReady}>
      <AppClientsPorts>{children}</AppClientsPorts>
    </HostReadinessContext.Provider>
  );
}

export interface ConnectionProvidersProps {
  /**
   * Override the registry for this subtree. Tests pass one holding an in-memory provider; the app
   * root passes none and gets a fresh registry the LiveKit provider registers itself into.
   */
  registry?: ConnectionProviderRegistry;
  children: ReactNode;
}

/** Provide a connection-provider registry to the component subtree. Mount once near the app root. */
export function ConnectionProviders({ registry, children }: ConnectionProvidersProps) {
  const ownRef = useRef<ConnectionProviderRegistry | null>(null);
  ownRef.current ??= new ConnectionProviderRegistry();
  const value = registry ?? ownRef.current;
  return (
    <ConnectionProviderContext.Provider value={value}>
      <AppClientsPorts>{children}</AppClientsPorts>
    </ConnectionProviderContext.Provider>
  );
}

/**
 * The registry in scope, or an empty one when no {@link ConnectionProviders} wraps this component.
 *
 * The empty registry is not an error path: it is what a component rendered outside the provider
 * gets, and it resolves every host to `null` — the same shape as "no host selected yet".
 *
 * The empty one is per component instance, exactly as `useHttpTransport`'s fallback transport is:
 * a module-level one would be shared mutable state that a registration in one test leaked into the
 * next. A component that both registers a wire and provides it onwards (`LiveKitConnections`) can
 * therefore read its registry here and re-provide it, and end up with the app root's registry when
 * there is one and a private one when there is not.
 */
export function useConnectionProviders(): ConnectionProviderRegistry {
  const inScope = useContext(ConnectionProviderContext);
  const ownRef = useRef<ConnectionProviderRegistry | null>(null);
  // Only when there is nothing in scope, so a component under a provider does not carry a second,
  // unreachable registry around for its lifetime.
  if (inScope) return inScope;
  ownRef.current ??= new ConnectionProviderRegistry();
  return ownRef.current;
}
