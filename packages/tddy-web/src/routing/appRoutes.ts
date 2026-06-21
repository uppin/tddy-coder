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

/** Path for the sessions drawer screen and its deep links. */
export const SESSIONS_DRAWER_ROUTE = "/sessions";

/**
 * Returns true for `/sessions` (the drawer root) and `/sessions/:id` (deep links).
 * Does NOT match `/sessions-extra` or other paths that merely start with `/sessions`.
 */
export function isSessionsDrawerPath(pathname: string): boolean {
  if (pathname === SESSIONS_DRAWER_ROUTE) {
    return true;
  }
  const prefix = `${SESSIONS_DRAWER_ROUTE}/`;
  if (pathname.startsWith(prefix)) {
    const segment = pathname.slice(prefix.length);
    return segment !== "" && !segment.includes("/");
  }
  return false;
}

/** Builds a `/sessions/:id` deep-link path for the given session id. */
export function sessionsDrawerPathForSession(sessionId: string): string {
  return `${SESSIONS_DRAWER_ROUTE}/${encodeURIComponent(sessionId)}`;
}

/**
 * Extracts the session id from a `/sessions/:id` pathname.
 * Returns `null` for `/sessions` (no segment) or non-matching paths.
 */
export function parseSessionsDrawerSessionId(pathname: string): string | null {
  const prefix = `${SESSIONS_DRAWER_ROUTE}/`;
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
