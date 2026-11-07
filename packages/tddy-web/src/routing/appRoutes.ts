/**
 * Canonical URL helpers for tddy-web session routing (terminal deep links, home, OAuth callback).
 *
 * Pure string rules only — no DOM, no React. `appLocation.ts` owns the params half of the grammar;
 * `useAppLocation.ts` owns the browser side.
 */

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

export {
  SESSIONS_NEW_ROUTE,
  isSessionsNewPath,
  parseSessionsProjectHomeSetup,
  parseSessionsProjectHomeSetupProjectId,
  isSessionsDrawerPath,
  parseSessionsDrawerSessionId,
  INSPECTOR_TAB_NAMES,
  isInspectorTabName,
  DORMANT_INSPECTOR_TAB_NAMES,
  isDormantInspectorTabName,
  MEDIA_INSPECTOR_TAB_NAMES,
  isMediaInspectorTabName,
} from "tddy-client-web";
export type {
  SessionsProjectHomeSetupRoute,
  InspectorTabName,
  DormantInspectorTabName,
  MediaInspectorTabName,
} from "tddy-client-web";
