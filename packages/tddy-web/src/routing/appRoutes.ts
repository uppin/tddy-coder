/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 */

/** Path prefix for the daemon-managed terminal view (one segment: session id). */
export const TERMINAL_SESSION_ROUTE_PREFIX = "/terminal";

export function terminalPathForSessionId(sessionId: string): string {
  return `${TERMINAL_SESSION_ROUTE_PREFIX}/${encodeURIComponent(sessionId)}`;
}

/**
 * Canonical path for terminal deep links. Must stay aligned with {@link terminalPathForSessionId}
 * when optional presentation query/hash is added (PRD).
 */
export function terminalDeepLinkSessionPath(sessionId: string): string {
  const path = terminalPathForSessionId(sessionId);
  console.debug("[tddy][appRoutes] terminalDeepLinkSessionPath", { path });
  return path;
}

export function parseTerminalSessionIdFromPathname(pathname: string): string | null {
  const prefix = `${TERMINAL_SESSION_ROUTE_PREFIX}/`;
  if (!pathname.startsWith(prefix)) {
    return null;
  }
  const segment = pathname.slice(prefix.length);
  if (segment === "" || segment.includes("/")) {
    return null;
  }
  try {
    return decodeURIComponent(segment);
  } catch {
    return null;
  }
}

export function isSessionListPath(pathname: string): boolean {
  return pathname === "/";
}

export function isAuthCallbackPath(pathname: string): boolean {
  return pathname === "/auth/callback";
}

/** Canonical path for the RPC Playground screen. */
export const RPC_PLAYGROUND_ROUTE = "/rpc-playground";

export function isRpcPlaygroundPath(pathname: string): boolean {
  return pathname === RPC_PLAYGROUND_ROUTE;
}

/** Canonical path for the VM management screen. */
export const VMS_ROUTE = "/vms";

export function isVmsPath(pathname: string): boolean {
  return pathname === VMS_ROUTE;
}
