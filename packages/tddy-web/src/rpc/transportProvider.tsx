/**
 * RPC transport provider — dependency-injection seam for ConnectRPC clients.
 *
 * Wrap the application root (or a component subtree) in `RpcTransportProvider`
 * to override the HTTP and/or LiveKit transport used by all RPC hooks.  The
 * default values reproduce the existing production behaviour exactly.
 *
 * Production usage (app root in index.tsx):
 * ```tsx
 * <RpcTransportProvider>
 *   <App />
 * </RpcTransportProvider>
 * ```
 *
 * Test usage (bun:test or Cypress component test):
 * ```tsx
 * const backend = anInMemoryRpcBackend()
 *   .onUnary(AuthService.method.getAuthStatus, () => ({ isAuthenticated: false }));
 *
 * render(
 *   <RpcTransportProvider httpTransport={backend.transport()}>
 *     <ComponentUnderTest />
 *   </RpcTransportProvider>
 * );
 * ```
 */

import { createContext, useContext, useMemo, useRef, type ReactNode } from "react";
import { createClient, type Client, type Transport } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import type { DescService } from "@bufbuild/protobuf";
import { createLiveKitTransport } from "tddy-livekit-web";
import type { Room } from "livekit-client";
import { TrafficMeterRegistry } from "./trafficMeter";
import { createTrafficInterceptor } from "./httpTrafficInterceptor";

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

/**
 * Factory for the production HTTP transport (binary Connect protocol, same-origin /rpc).
 * When a registry is provided, attaches a traffic-metering interceptor. Exported so a test can
 * point a `liveKitFactory` override at the same HTTP transport its `cy.intercept`s already expect,
 * without needing a real (or fully faked) LiveKit data-channel connection.
 */
export function createDefaultHttpTransport(registry?: TrafficMeterRegistry): Transport {
  const interceptors = registry ? [createTrafficInterceptor(registry.get("http"))] : [];
  return createConnectTransport({
    baseUrl:
      typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
    interceptors,
  });
}

/** Options forwarded to the LiveKit transport factory. */
export interface LiveKitTransportOptions {
  /** Enable debug logging in the transport. */
  debug?: boolean;
}

/** True when the active debug mask enables `tddy:rpc*` — turns on the LiveKit transport's per-RPC
 *  console logging (publish/unary/stream/response with service/method) so client↔server RPC routing
 *  is visible. The mask is stored under the `debug`-package key; set via `dev.daemon.yaml` `debug`
 *  (served at /api/config) or DevTools `localStorage.debug = 'tddy:rpc:*'`. */
function rpcDebugActive(): boolean {
  try {
    return (window.localStorage.getItem("debug") ?? "").includes("tddy:rpc");
  } catch {
    return false;
  }
}

/** Factory for the production LiveKit transport. */
function createDefaultLiveKitTransport(
  room: Room,
  targetIdentity: string,
  options?: LiveKitTransportOptions,
  registry?: TrafficMeterRegistry,
): Transport {
  return createLiveKitTransport({
    room,
    targetIdentity,
    debug: options?.debug ?? rpcDebugActive(),
    meter: registry?.get(room.name || "livekit"),
  });
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface RpcTransportContextValue {
  readonly httpTransport: Transport;
  readonly liveKitFactory: (room: Room, targetIdentity: string, options?: LiveKitTransportOptions) => Transport;
  readonly meterRegistry: TrafficMeterRegistry;
  /**
   * True when this provider was given an explicit `liveKitFactory` override (the test-double
   * seam described on `RpcTransportProviderProps.liveKitFactory`). Callers that need a real
   * connected `Room` to build a transport (the production default does) can use this to tell
   * "no room yet, but a test double that doesn't care" apart from "no room yet, for real."
   */
  readonly liveKitFactoryIsOverridden: boolean;
}

const RpcTransportContext = createContext<RpcTransportContextValue | null>(null);

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export interface RpcTransportProviderProps {
  /**
   * Override the HTTP transport used by all hooks in this subtree.
   * Defaults to `createConnectTransport({ baseUrl: ".../rpc", useBinaryFormat: true })`.
   */
  httpTransport?: Transport;
  /**
   * Override the LiveKit transport factory.
   * Defaults to `createLiveKitTransport({ room, targetIdentity })`.
   *
   * For tests, pass `() => backend.transport()` to route all LiveKit RPC through
   * the in-memory backend instead of a real LiveKit data channel.
   */
  liveKitFactory?: (room: Room, targetIdentity: string, options?: LiveKitTransportOptions) => Transport;
  children: ReactNode;
}

/**
 * Provide HTTP and LiveKit transports to the component subtree.
 *
 * Mount once near the app root in production.  In tests, mount around the
 * component under test and pass `httpTransport={backend.transport()}`.
 */
export function RpcTransportProvider({
  httpTransport,
  liveKitFactory,
  children,
}: RpcTransportProviderProps) {
  // Each provider instance owns its own meter registry — isolated from other
  // provider subtrees (prevents test pollution when multiple providers exist).
  const registryRef = useRef<TrafficMeterRegistry | null>(null);
  registryRef.current ??= new TrafficMeterRegistry();

  // Stable references — avoid rebuilding the context object on every render
  // when props haven't changed (the defaults are module-level constants).
  const httpRef = useRef<Transport | null>(null);
  if (httpTransport !== undefined) {
    // Caller supplied an explicit transport — use it directly.
    httpRef.current = httpTransport;
  } else if (httpRef.current === null) {
    // First render without an explicit transport — create the default once, with metering.
    httpRef.current = createDefaultHttpTransport(registryRef.current);
  }

  const registry = registryRef.current;
  const lkFactory = liveKitFactory ??
    ((room, targetIdentity, opts) => createDefaultLiveKitTransport(room, targetIdentity, opts, registry));

  const value: RpcTransportContextValue = {
    httpTransport: httpRef.current,
    liveKitFactory: lkFactory,
    meterRegistry: registry,
    liveKitFactoryIsOverridden: liveKitFactory !== undefined,
  };

  return (
    <RpcTransportContext.Provider value={value}>
      {children}
    </RpcTransportContext.Provider>
  );
}

// ---------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------

/**
 * Return the traffic meter registry for this component's context, or null when
 * no `RpcTransportProvider` wraps the component. Components that display meter
 * data should treat null as "no provider → show zeros".
 */
export function useTrafficMeterRegistry(): TrafficMeterRegistry | null {
  return useContext(RpcTransportContext)?.meterRegistry ?? null;
}

/**
 * Return the HTTP transport for this component's context.
 *
 * When no `RpcTransportProvider` wraps this component, a fresh default
 * transport is created and memoised per component instance — identical to the
 * pre-DI inline-factory behaviour. The fallback transport has no metering
 * interceptor (no registry is available without a provider).
 */
export function useHttpTransport(): Transport {
  const ctx = useContext(RpcTransportContext);
  // Stable fallback per component instance when no provider is present.
  const fallbackRef = useRef<Transport | null>(null);
  fallbackRef.current ??= createDefaultHttpTransport();
  return ctx?.httpTransport ?? fallbackRef.current;
}

/**
 * Create and memoize a ConnectRPC client bound to the HTTP transport in this context.
 *
 * Equivalent to `useMemo(() => createClient(Service, useHttpTransport()), [transport])`.
 */
export function useHttpClient<S extends DescService>(service: S): Client<S> {
  const transport = useHttpTransport();
  // `service` is always a module-level constant so its reference is stable;
  // including it in the dep array is correct without cost.
  return useMemo(() => createClient(service, transport), [service, transport]);
}

/**
 * Return the LiveKit transport factory for this component's context.
 *
 * Usage:
 * ```ts
 * const liveKitFactory = useLiveKitTransportFactory();
 * // ...after connecting to a room:
 * const transport = liveKitFactory(room, serverIdentity, { debug: true });
 * const client = createClient(TerminalService, transport);
 * ```
 */
export function useLiveKitTransportFactory(): (room: Room, targetIdentity: string, options?: LiveKitTransportOptions) => Transport {
  const ctx = useContext(RpcTransportContext);
  // Fallback (no provider): no meter registry available, so no metering.
  return ctx?.liveKitFactory ?? createDefaultLiveKitTransport;
}

/**
 * True when the LiveKit transport factory in scope is a test double (an explicit
 * `RpcTransportProvider liveKitFactory` override), false for the real production default.
 *
 * The production default (`createDefaultLiveKitTransport`) requires a genuinely connected
 * `Room` and crashes without one; a test double (e.g. one that routes to an in-memory RPC
 * backend) is free to ignore its `room`/`targetIdentity` arguments entirely. Callers that may
 * be invoked before a real `Room` exists (see `usePresenterChat`) use this to build a client
 * via the test double regardless, while still refusing to call the unsafe production default
 * without a room.
 */
export function useLiveKitTransportFactoryIsOverridden(): boolean {
  const ctx = useContext(RpcTransportContext);
  return ctx?.liveKitFactoryIsOverridden ?? false;
}

/**
 * Build and memoize the LiveKit transport for the given room and participant. Returns `null`
 * when either `room` or `targetIdentity` is not yet available.
 *
 * Use when you need the raw transport (e.g. for a generic `invokeRpc` call alongside a
 * service-specific client). When you only need a specific service client, prefer
 * {@link useLiveKitClient} instead.
 */
export function useLiveKitTransport(
  room: Room | null | undefined,
  targetIdentity: string | null | undefined,
  options?: LiveKitTransportOptions,
): Transport | null {
  const factory = useLiveKitTransportFactory();
  return useMemo(
    () => (room && targetIdentity ? factory(room, targetIdentity, options) : null),
    // options intentionally omitted — pass a stable reference if needed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [factory, room, targetIdentity],
  );
}

/**
 * Create and memoize a ConnectRPC client over the LiveKit transport for the given room and
 * participant. Returns `null` when either `room` or `targetIdentity` is not yet available.
 *
 * Use when both values are known at render time (e.g. a `room` from `useCommonRoom` and a
 * selected participant ID from state). When the room is local to a `useEffect` callback,
 * call `useLiveKitTransportFactory()` instead and construct the client inside the effect.
 */
export function useLiveKitClient<S extends DescService>(
  service: S,
  room: Room | null | undefined,
  targetIdentity: string | null | undefined,
  options?: LiveKitTransportOptions,
): Client<S> | null {
  const factory = useLiveKitTransportFactory();
  return useMemo(
    () => (room && targetIdentity ? createClient(service, factory(room, targetIdentity, options)) : null),
    // options is intentionally omitted — callers should pass a stable reference if needed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [service, factory, room, targetIdentity],
  );
}
