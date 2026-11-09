/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 *
 * Pure string rules only — no DOM, no React. `appLocation.ts` owns the params half of the grammar;
 * `useAppLocation.ts` owns the browser side.
 */

import {
  SESSIONS_DRAWER_ROUTE,
  SESSIONS_PROJECT_HOME_SETUP_ROUTE,
  PROJECT_HOME_REPLACE_SEGMENT,
  decodeSegment,
  singleSegmentAfter,
} from "tddy-client-web";

export {
  SESSIONS_DRAWER_ROUTE,
  SESSIONS_PROJECT_HOME_SETUP_ROUTE,
  sessionsDrawerPathForSession,
  sessionsProjectHomeSetupPath,
} from "tddy-client-web";


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

/**
 * Canonical path for the Accounts screen — the credentials a daemon holds, one row per linked
 * account, grouped by provider.
 */
export const ACCOUNTS_ROUTE = "/accounts";

/**
 * Returns true only for `/accounts`. Does NOT match `/accounts-archive` or any other path that
 * merely starts with `/accounts` — the screen owns exactly one path, as Projects and Hosts do.
 */
export function isAccountsPath(pathname: string): boolean {
  return pathname === ACCOUNTS_ROUTE;
}

/** Canonical path for the Hosts screen — every host tddy has seen, reachable or not. */
export const HOSTS_ROUTE = "/hosts";

export function isHostsPath(pathname: string): boolean {
  return pathname === HOSTS_ROUTE;
}

/** Canonical path for the Models & Agents screen. */
export const MODELS_ROUTE = "/models";

export function isModelsPath(pathname: string): boolean {
  return pathname === MODELS_ROUTE;
}

/** Canonical path for the daemon settings screen. */
export const SETTINGS_ROUTE = "/settings";

export function isSettingsPath(pathname: string): boolean {
  return pathname === SETTINGS_ROUTE;
}

/**
 * Path for the create-session pane. `new` is **reserved** as a session-id segment: session ids are
 * UUIDs, so the reservation cannot collide with a real session.
 */
export const SESSIONS_NEW_ROUTE = `${SESSIONS_DRAWER_ROUTE}/new`;

export function isSessionsNewPath(pathname: string): boolean {
  return pathname === SESSIONS_NEW_ROUTE;
}

/** Which project the setup pane is open for, and whether it replaces a main already set up. */
export interface SessionsProjectHomeSetupRoute {
  projectId: string;
  replacing: boolean;
}

/**
 * Reads `/sessions/project-home/:projectId` and `/sessions/project-home/:projectId/replace`;
 * `null` for any other path.
 */
export function parseSessionsProjectHomeSetup(pathname: string): SessionsProjectHomeSetupRoute | null {
  const plain = decodeSegment(singleSegmentAfter(pathname, SESSIONS_PROJECT_HOME_SETUP_ROUTE));
  if (plain !== null) return { projectId: plain, replacing: false };
  // The project id is encoded, so a literal `/` can only be the one before the replace segment.
  const replaceSuffix = `/${PROJECT_HOME_REPLACE_SEGMENT}`;
  if (!pathname.endsWith(replaceSuffix)) return null;
  const setupPath = pathname.slice(0, -replaceSuffix.length);
  const projectId = decodeSegment(singleSegmentAfter(setupPath, SESSIONS_PROJECT_HOME_SETUP_ROUTE));
  return projectId === null ? null : { projectId, replacing: true };
}

/** Extracts the project id from either setup path; `null` for any other path. */
export function parseSessionsProjectHomeSetupProjectId(pathname: string): string | null {
  return parseSessionsProjectHomeSetup(pathname)?.projectId ?? null;
}

/**
 * Returns true for `/sessions` (the drawer root), `/sessions/:id` (deep links), `/sessions/new` and
 * `/sessions/project-home/:projectId[/replace]` — every path the sessions drawer screen owns.
 * Does NOT match `/sessions-extra` or other paths that merely start with `/sessions`.
 */
export function isSessionsDrawerPath(pathname: string): boolean {
  return (
    pathname === SESSIONS_DRAWER_ROUTE ||
    singleSegmentAfter(pathname, SESSIONS_DRAWER_ROUTE) !== null ||
    parseSessionsProjectHomeSetupProjectId(pathname) !== null
  );
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
  "activity",
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

/**
 * The inspector tabs that exist only for a stopped session: its recorded transcript (`activity`),
 * which has no other route once the Agent tab shows the greyed terminal output.
 */
export const DORMANT_INSPECTOR_TAB_NAMES = ["activity"] as const;

export type DormantInspectorTabName = (typeof DORMANT_INSPECTOR_TAB_NAMES)[number];

/** Whether `value` names one of the tabs that exists only for a stopped session. */
export function isDormantInspectorTabName(value: string): value is DormantInspectorTabName {
  return (DORMANT_INSPECTOR_TAB_NAMES as readonly string[]).includes(value);
}

/**
 * The inspector tabs that exist only on a connection that can carry video tracks.
 *
 * The VNC and screen-sharing bridges publish their frames as a LiveKit video track into the
 * session's own room (`vnc_service.rs` reads the session's `livekit_room`), so a session reached
 * over a wire that carries no tracks has no picture to show and these two tabs are absent rather
 * than dead. Declared here beside {@link INSPECTOR_TAB_NAMES} for the same reason that list is:
 * an `inspector=` param is normalised from a pure module.
 */
export const MEDIA_INSPECTOR_TAB_NAMES = ["vnc", "screen-sharing"] as const;

export type MediaInspectorTabName = (typeof MEDIA_INSPECTOR_TAB_NAMES)[number];

/** Whether `value` names one of the tabs that needs the `media` capability. */
export function isMediaInspectorTabName(value: string): value is MediaInspectorTabName {
  return (MEDIA_INSPECTOR_TAB_NAMES as readonly string[]).includes(value);
}
