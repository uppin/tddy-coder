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

import { createContext, useContext, useRef, type ReactNode } from "react";
import type { Transport } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { createLiveKitTransport } from "tddy-livekit-web";
import type { Room } from "livekit-client";

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

/** Factory for the production HTTP transport (binary Connect protocol, same-origin /rpc). */
function createDefaultHttpTransport(): Transport {
  return createConnectTransport({
    baseUrl:
      typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
}

/** Options forwarded to the LiveKit transport factory. */
export interface LiveKitTransportOptions {
  /** Enable debug logging in the transport. */
  debug?: boolean;
}

/** Factory for the production LiveKit transport. */
function createDefaultLiveKitTransport(
  room: Room,
  targetIdentity: string,
  options?: LiveKitTransportOptions,
): Transport {
  return createLiveKitTransport({ room, targetIdentity, debug: options?.debug });
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

interface RpcTransportContextValue {
  readonly httpTransport: Transport;
  readonly liveKitFactory: (room: Room, targetIdentity: string, options?: LiveKitTransportOptions) => Transport;
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
  // Stable references — avoid rebuilding the context object on every render
  // when props haven't changed (the defaults are module-level constants).
  const httpRef = useRef<Transport | null>(null);
  if (httpTransport !== undefined) {
    // Caller supplied an explicit transport — use it directly.
    httpRef.current = httpTransport;
  } else if (httpRef.current === null) {
    // First render without an explicit transport — create the default once.
    httpRef.current = createDefaultHttpTransport();
  }

  const lkFactory = liveKitFactory ?? createDefaultLiveKitTransport;

  const value: RpcTransportContextValue = {
    httpTransport: httpRef.current,
    liveKitFactory: lkFactory,
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
 * Return the HTTP transport for this component's context.
 *
 * When no `RpcTransportProvider` wraps this component, a fresh default
 * transport is created and memoised per component instance — identical to the
 * pre-DI inline-factory behaviour.
 */
export function useHttpTransport(): Transport {
  const ctx = useContext(RpcTransportContext);
  // Stable fallback per component instance when no provider is present.
  const fallbackRef = useRef<Transport | null>(null);
  fallbackRef.current ??= createDefaultHttpTransport();
  return ctx?.httpTransport ?? fallbackRef.current;
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
  return ctx?.liveKitFactory ?? createDefaultLiveKitTransport;
}
