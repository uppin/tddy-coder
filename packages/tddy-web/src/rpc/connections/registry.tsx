/**
 * The connection-provider registry and the hooks that resolve a host through it.
 *
 * `tddy-web` imports no provider. A provider is registered by whoever knows the wire — the LiveKit
 * one from the app root, the IPC one from the desktop build — and every call site resolves a host
 * through this registry instead of naming a `Room` and a participant identity.
 *
 * Mirrors the shape of `../transportProvider`: a React context with sensible empty defaults rather
 * than a throw, because every consumer already guards a null client. That guard is what makes
 * LiveKit optional: with no provider registered, `useHostConnection` returns `null` and each screen
 * renders its existing "not connected" state instead of failing.
 */

import { createContext, useRef, type ReactNode } from "react";
import type { Client } from "@connectrpc/connect";
import type { DescService } from "@bufbuild/protobuf";
import type { ConnectionProvider, HostConnection } from "./types";

/**
 * The registered providers, in precedence order.
 *
 * Precedence is registration order, first match wins, and it is load-bearing exactly once: the
 * desktop build registers its IPC provider ahead of the LiveKit one, so its own host is reached
 * in-process even when a common room is configured and could also reach it. Expressing that as
 * order rather than as a preference setting is why there is no `preferIpc` flag anywhere.
 */
export class ConnectionProviderRegistry {
  private readonly providers: ConnectionProvider[] = [];

  /** Append `provider`. A provider already registered under the same id replaces the earlier one. */
  register(provider: ConnectionProvider): void {
    // TODO(connection-model): implement
    throw new Error(`ConnectionProviderRegistry.register(${provider.id}) is not implemented yet`);
  }

  /**
   * The first registered provider that can reach `hostId`, or `null` when none can.
   *
   * `null` is a normal answer — see `ConnectionProvider.connectHost`.
   */
  connectHost(hostId: string): HostConnection | null {
    // TODO(connection-model): implement
    throw new Error(`ConnectionProviderRegistry.connectHost(${hostId}) is not implemented yet`);
  }

  /** The ids of the registered providers, in precedence order. Diagnostics and tests. */
  providerIds(): readonly string[] {
    // TODO(connection-model): implement
    throw new Error("ConnectionProviderRegistry.providerIds is not implemented yet");
  }
}

const ConnectionProviderContext = createContext<ConnectionProviderRegistry | null>(null);

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
      {children}
    </ConnectionProviderContext.Provider>
  );
}

/**
 * The registry in scope, or an empty one when no {@link ConnectionProviders} wraps this component.
 *
 * The empty registry is not an error path: it is what a component rendered outside the provider
 * gets, and it resolves every host to `null` — the same shape as "no host selected yet".
 */
export function useConnectionProviders(): ConnectionProviderRegistry {
  // TODO(connection-model): implement
  throw new Error("useConnectionProviders is not implemented yet");
}

/**
 * The connection to `hostId`, or `null` when no registered provider can reach it — including when
 * `hostId` is `null` because no host is selected yet.
 *
 * Memoised on `hostId` and the registry, so a connection's identity is as stable as its routing.
 */
export function useHostConnection(hostId: string | null): HostConnection | null {
  // TODO(connection-model): implement
  throw new Error(`useHostConnection(${hostId}) is not implemented yet`);
}

/**
 * A client for `service` on `hostId`, or `null` when the host is unreachable or unselected.
 *
 * The daemon-level equivalent of `useHttpClient` from `../transportProvider`, and the replacement
 * for `useLiveKitClient(service, room, daemonRpcIdentity(hostId))`. Call sites must guard the null,
 * exactly as they guard it today.
 */
export function useHostClient<S extends DescService>(
  service: S,
  hostId: string | null,
): Client<S> | null {
  // TODO(connection-model): implement
  throw new Error(`useHostClient(${service.typeName}, ${hostId}) is not implemented yet`);
}
