/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 */

/** Path prefix for the daemon-managed terminal view (one segment: session id). */
export const TERMINAL_SESSION_ROUTE_PREFIX = "/terminal";

/** Path prefix for the per-project session list screen (one encoded segment: stable project row key). */
export const PROJECT_ROW_ROUTE_PREFIX = "/project";

export function projectPathForRowKey(rowKey: string): string {
  const path = `${PROJECT_ROW_ROUTE_PREFIX}/${encodeURIComponent(rowKey)}`;
  console.debug("[tddy][appRoutes] projectPathForRowKey", { rowKey, path });
  return path;
}

/**
 * Returns the decoded project row key for `/project/:encodedKey`, or null when the path is not a
 * single-segment project URL (including `/terminal/*` and extra path segments).
 */
export function parseProjectRowKeyFromPathname(pathname: string): string | null {
  const prefix = `${PROJECT_ROW_ROUTE_PREFIX}/`;
  if (!pathname.startsWith(prefix)) {
    return null;
  }
  const segment = pathname.slice(prefix.length);
  if (segment === "" || segment.includes("/")) {
    console.debug("[tddy][appRoutes] parseProjectRowKeyFromPathname: reject empty or multi-segment", {
      pathname,
    });
    return null;
  }
  try {
    const decoded = decodeURIComponent(segment);
    console.debug("[tddy][appRoutes] parseProjectRowKeyFromPathname", { pathname, decoded });
    return decoded;
  } catch {
    console.info("[tddy][appRoutes] parseProjectRowKeyFromPathname: decode failed", { pathname });
    return null;
  }
}

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
