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

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useSyncExternalStore,
  type ReactNode,
} from "react";
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
 *
 * The registry is mutable and long-lived — a wire registers itself when it comes up, and comes up
 * again on a different connection later. So it is also an observable store (`subscribe`/{@link
 * revision}, the shape `sessionNotificationRegistry` already uses): a call site that has no other
 * reason to re-render still learns that the host it could not reach a moment ago is now reachable.
 */
export class ConnectionProviderRegistry {
  private readonly providers: ConnectionProvider[] = [];
  private readonly listeners = new Set<() => void>();
  private currentRevision = 0;
  private notifyScheduled = false;

  /** Append `provider`. A provider already registered under the same id replaces the earlier one. */
  register(provider: ConnectionProvider): void {
    const existing = this.providers.findIndex((p) => p.id === provider.id);
    if (existing >= 0) {
      // Re-registering the same instance is what a wire does on every render of the component that
      // offers it; treating that as a change would notify every consumer into rebuilding clients
      // that did not move.
      if (this.providers[existing] === provider) return;
      // Replaced where it stood, not appended: precedence is a deployment's decision, and a wire
      // that re-registers on reconnect must not overtake — or fall behind — the wires it was
      // ordered against.
      this.providers[existing] = provider;
    } else {
      this.providers.push(provider);
    }
    this.currentRevision += 1;
    this.scheduleNotify();
  }

  /**
   * The first registered provider that can reach `hostId`, or `null` when none can.
   *
   * `null` is a normal answer — see `ConnectionProvider.connectHost`.
   */
  connectHost(hostId: string): HostConnection | null {
    for (const provider of this.providers) {
      const connection = provider.connectHost(hostId);
      if (connection) return connection;
    }
    return null;
  }

  /** The ids of the registered providers, in precedence order. Diagnostics and tests. */
  providerIds(): readonly string[] {
    return this.providers.map((provider) => provider.id);
  }

  /**
   * Tell the observers, once, after the current task.
   *
   * Deferred because a wire registers itself while the component that owns it renders — that is the
   * only moment early enough for the subtree's first paint to resolve its hosts — and a render may
   * not update other components. By the time the notification lands they have usually rendered with
   * the new routing already, read the same {@link revision} they were about to read anyway, and
   * `useSyncExternalStore` drops the update. What it does catch is the case that motivates the
   * subscription at all: a subtree that would otherwise not re-render, because the element it was
   * handed did not change.
   */
  private scheduleNotify(): void {
    if (this.notifyScheduled || this.listeners.size === 0) return;
    this.notifyScheduled = true;
    queueMicrotask(() => {
      this.notifyScheduled = false;
      for (const listener of this.listeners) listener();
    });
  }

  /** Observe registrations. Returns the unsubscribe. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * How many times the provider list has changed — the `useSyncExternalStore` snapshot.
   *
   * A counter rather than the provider array itself, because the array is mutated in place: a
   * snapshot has to be a value that compares equal until something actually changed.
   */
  revision(): number {
    return this.currentRevision;
  }
}

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
const HostReadinessContext = createContext<((hostId: string) => boolean) | null>(null);

/** Provide the readiness predicate for hosts resolved below. Identity changes when readiness does. */
export function HostReadiness({
  isReady,
  children,
}: {
  isReady: (hostId: string) => boolean;
  children: ReactNode;
}) {
  return <HostReadinessContext.Provider value={isReady}>{children}</HostReadinessContext.Provider>;
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
      {children}
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

/**
 * Resolve hosts named at call time, for a caller that cannot name them at render time.
 *
 * A fan-out reads a host list that changes, and a form addresses the host the operator just picked;
 * neither can spend a hook per host. They take this resolver instead and call it inside their
 * effect or callback. Its identity changes exactly when the routing does — a provider registering,
 * or re-registering with a new wire — so it is the dependency that replaces `[room, liveKitFactory]`
 * in those callers' dependency arrays, and it re-runs their reads for the same reasons those did.
 */
export function useHostConnector(): (hostId: string | null) => HostConnection | null {
  // Not compiled: the memo below is keyed on `revision`, which the resolver never reads. The React
  // Compiler memoises on what a body reads, so it would drop `revision`, keep the resolver from before
  // the common room joined, and leave every `useHostConnection` returning `null` for the page's life.
  "use no memo";
  const registry = useConnectionProviders();
  const isReady = useContext(HostReadinessContext);
  const subscribe = useCallback((listener: () => void) => registry.subscribe(listener), [registry]);
  const revision = useSyncExternalStore(subscribe, () => registry.revision());
  // `revision` is a dependency, not an input — the resolver reads the registry's providers at the
  // moment it is called. Re-deriving on it is what changes this function's identity when a wire
  // registers, and so what re-runs the effects and callbacks that hold it.
  return useMemo(
    () => (hostId: string | null) =>
      hostId && (isReady?.(hostId) ?? true) ? registry.connectHost(hostId) : null,
    [registry, revision, isReady],
  );
}

/**
 * The connection to `hostId`, or `null` when no registered provider can reach it — including when
 * `hostId` is `null` because no host is selected yet, and when the host is not ready yet (see
 * {@link HostReadiness}): a waiting host is asked nothing.
 *
 * Memoised on `hostId` and the registry, so a connection's identity is as stable as its routing.
 */
export function useHostConnection(hostId: string | null): HostConnection | null {
  const connect = useHostConnector();
  return useMemo(() => connect(hostId), [connect, hostId]);
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
  const connection = useHostConnection(hostId);
  // The connection memoises the client per service, so this returns one instance for as long as the
  // routing holds — a consumer keying an effect on it does not tear its stream down on every render.
  return useMemo(() => connection?.clientFor(service) ?? null, [connection, service]);
}
