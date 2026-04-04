/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 */

/** Path prefix for the daemon-managed terminal view (one segment: session id). */
export const TERMINAL_SESSION_ROUTE_PREFIX = "/terminal";

export function terminalPathForSessionId(sessionId: string): string {
  return `${TERMINAL_SESSION_ROUTE_PREFIX}/${encodeURIComponent(sessionId)}`;
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
