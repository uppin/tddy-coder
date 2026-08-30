/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 *
 * Pure string rules only — no DOM, no React. `appLocation.ts` owns the params half of the grammar;
 * `useAppLocation.ts` owns the browser side.
 */

/**
 * The single path segment following `prefix`, or `null` when `pathname` is not
 * `{prefix}/{one-segment}`. Raw (still percent-encoded) so an encoded `/` inside a segment cannot
 * be mistaken for a separator.
 */
function singleSegmentAfter(pathname: string, prefix: string): string | null {
  const withSlash = `${prefix}/`;
  if (!pathname.startsWith(withSlash)) {
    return null;
  }
  const segment = pathname.slice(withSlash.length);
  return segment === "" || segment.includes("/") ? null : segment;
}

/** Percent-decode a raw path segment, treating a malformed escape as "no match". */
function decodeSegment(segment: string | null): string | null {
  if (segment === null) {
    return null;
  }
  try {
    return decodeURIComponent(segment);
  } catch {
    return null;
  }
}

/** Path prefix for legacy terminal deep links (one segment: session id). */
export const TERMINAL_SESSION_ROUTE_PREFIX = "/terminal";

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

export function isAuthCallbackPath(pathname: string): boolean {
  return pathname === "/auth/callback";
}

/** Canonical path for the RPC Playground screen. */
export const RPC_PLAYGROUND_ROUTE = "/rpc-playground";

export function isRpcPlaygroundPath(pathname: string): boolean {
  return pathname === RPC_PLAYGROUND_ROUTE;
}

/** Canonical path for the Tasks management screen. */
export const TASKS_ROUTE = "/tasks";

/**
 * Returns true for `/tasks` (the drawer root) and `/tasks/:taskId` (deep links).
 * Does NOT match `/tasks-archive` or other paths that merely start with `/tasks`.
 */
export function isTasksPath(pathname: string): boolean {
  return pathname === TASKS_ROUTE || singleSegmentAfter(pathname, TASKS_ROUTE) !== null;
}

/** Builds a `/tasks/:taskId` deep-link path for the given task id. */
export function tasksPathForTask(taskId: string): string {
  return `${TASKS_ROUTE}/${encodeURIComponent(taskId)}`;
}

/**
 * Extracts the task id from a `/tasks/:taskId` pathname.
 * Returns `null` for `/tasks` (no segment) or non-matching paths.
 */
export function parseTaskId(pathname: string): string | null {
  return decodeSegment(singleSegmentAfter(pathname, TASKS_ROUTE));
}

/** Canonical path for the VM management screen. */
export const VMS_ROUTE = "/vms";

export function isVmsPath(pathname: string): boolean {
  return pathname === VMS_ROUTE;
}

/** Canonical path for the LiveKit presence screen. */
export const LIVEKIT_ROUTE = "/livekit";

export function isLiveKitPath(pathname: string): boolean {
  return pathname === LIVEKIT_ROUTE;
}

/** Canonical path for the dedicated Projects screen. */
export const PROJECTS_ROUTE = "/projects";

export function isProjectsPath(pathname: string): boolean {
  return pathname === PROJECTS_ROUTE;
}

/** Canonical path for the Models & Agents screen. */
export const MODELS_ROUTE = "/models";

export function isModelsPath(pathname: string): boolean {
  return pathname === MODELS_ROUTE;
}

/** Path for the sessions drawer screen and its deep links. */
export const SESSIONS_DRAWER_ROUTE = "/sessions";

/**
 * Path for the create-session pane. `new` is **reserved** as a session-id segment: session ids are
 * UUIDs, so the reservation cannot collide with a real session.
 */
export const SESSIONS_NEW_ROUTE = `${SESSIONS_DRAWER_ROUTE}/new`;

export function isSessionsNewPath(pathname: string): boolean {
  return pathname === SESSIONS_NEW_ROUTE;
}

/**
 * Returns true for `/sessions` (the drawer root), `/sessions/:id` (deep links) and `/sessions/new` —
 * every path the sessions drawer screen owns.
 * Does NOT match `/sessions-extra` or other paths that merely start with `/sessions`.
 */
export function isSessionsDrawerPath(pathname: string): boolean {
  return (
    pathname === SESSIONS_DRAWER_ROUTE ||
    singleSegmentAfter(pathname, SESSIONS_DRAWER_ROUTE) !== null
  );
}

/** Builds a `/sessions/:id` deep-link path for the given session id. */
export function sessionsDrawerPathForSession(sessionId: string): string {
  return `${SESSIONS_DRAWER_ROUTE}/${encodeURIComponent(sessionId)}`;
}

/**
 * Extracts the selected session id from `/sessions/:id`.
 * Returns `null` for `/sessions`, the reserved `/sessions/new`, and non-matching paths.
 */
export function parseSessionsDrawerSessionId(pathname: string): string | null {
  if (isSessionsNewPath(pathname)) {
    return null;
  }
  return decodeSegment(singleSegmentAfter(pathname, SESSIONS_DRAWER_ROUTE));
}

/**
 * The session inspector's tabs, in strip order. Declared here rather than in `InspectorTabs.tsx`
 * so the `inspector=` param can be validated from a pure module (importing the `.tsx` component
 * would drag React's JSX runtime into `bun test`).
 */
export const INSPECTOR_TAB_NAMES = [
  "details",
  "tools",
  "agents",
  "usage",
  "worktree",
  "files",
  "vnc",
  "screen-sharing",
] as const;

export type InspectorTabName = (typeof INSPECTOR_TAB_NAMES)[number];

/** Whether an `inspector=` param value names a real tab. An unknown value degrades to the default. */
export function isInspectorTabName(value: string): value is InspectorTabName {
  return (INSPECTOR_TAB_NAMES as readonly string[]).includes(value);
}
