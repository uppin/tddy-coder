/**
 * The app's location, modelled as a path plus a hash-local query string.
 *
 * Every navigable selection in tddy-web lives in the URL hash — `#/<path>?<params>` — so a copied
 * address bar reproduces what the operator is looking at. This module is the only place that knows
 * how that string is spelled: percent-encoding, the `?`-inside-the-hash split, and which params
 * survive a screen change. It is pure (no DOM); `useAppLocation` owns the browser side.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 */

/** Param carried on every path: the selected daemon's instance id. */
export const PARAM_HOST = "host";
/** `#/sessions/:id` — the open inspector's tab. Absent means the inspector is closed. */
export const PARAM_INSPECTOR = "inspector";
/** `#/sessions/:id` — `"1"` when the inspector is expanded rather than docked. */
export const PARAM_FULL = "full";
/** `#/sessions/:id` — `"1"` when the worktree Code split pane is open. */
export const PARAM_CODE = "code";
/** `#/tasks/:taskId` — the selected output-channel tab. */
export const PARAM_CHANNEL = "channel";
/** `#/worktrees` — the project filter. */
export const PARAM_PROJECT = "project";
/** `#/rpc-playground` — the addressed LiveKit participant. */
export const PARAM_PARTICIPANT = "participant";
/** `#/rpc-playground` — the selected service. */
export const PARAM_SERVICE = "service";
/** `#/rpc-playground` — the selected method. */
export const PARAM_METHOD = "method";

/**
 * Params that outlive a screen change. Everything else is scoped to the screen that set it and
 * would be meaningless — or actively misleading — carried onto the next one.
 */
const SCREEN_INDEPENDENT_PARAMS: readonly string[] = [PARAM_HOST];

export interface AppLocation {
  /** The hash path, always leading-slashed (`/sessions/abc`). */
  readonly path: string;
  /** The hash-local query params, decoded. */
  readonly params: Readonly<Record<string, string>>;
}

/**
 * Parse a location out of a hash. Accepts the value of `window.location.hash` (with or without the
 * leading `#`); an empty hash reads as the root path.
 */
export function parseAppLocation(hash: string): AppLocation {
  const raw = hash.startsWith("#") ? hash.slice(1) : hash;
  if (raw === "") {
    return { path: "/", params: {} };
  }
  const queryAt = raw.indexOf("?");
  if (queryAt === -1) {
    return { path: raw, params: {} };
  }
  const params: Record<string, string> = {};
  for (const [key, value] of new URLSearchParams(raw.slice(queryAt + 1))) {
    params[key] = value;
  }
  return { path: raw.slice(0, queryAt), params };
}

/** Render a location as a hash, including the leading `#`. */
export function formatAppLocation(location: AppLocation): string {
  const search = new URLSearchParams(location.params).toString();
  return search === "" ? `#${location.path}` : `#${location.path}?${search}`;
}

/**
 * Apply a param patch, returning a new location. A `null` value deletes the param; every other
 * value sets it. Params not named in the patch are untouched.
 */
export function withParams(
  location: AppLocation,
  patch: Readonly<Record<string, string | null>>,
): AppLocation {
  const params: Record<string, string> = { ...location.params };
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) {
      delete params[key];
    } else {
      params[key] = value;
    }
  }
  return { path: location.path, params };
}

/** The screen a path belongs to — its first segment (`/sessions/abc/add-agent` → `sessions`). */
function screenOf(path: string): string {
  return path.replace(/^\//, "").split("/")[0] ?? "";
}

/**
 * The root path of the screen a path belongs to (`/sessions/abc/add-agent` → `/sessions`).
 * Used to drop a sub-selection that the new context cannot resolve — switching host, for one.
 */
export function screenRootOf(path: string): string {
  return `/${screenOf(path)}`;
}

/**
 * Move to `path`, deciding what the params do.
 *
 * A **screen change** (a different first path segment) drops every screen-scoped param and keeps
 * only the screen-independent ones — an `inspector` tab means nothing on `#/tasks`. A move *within*
 * a screen (`/sessions/abc` → `/sessions/def`) keeps them all, so the inspector does not close
 * because the operator clicked the next row in the drawer.
 */
export function withPath(location: AppLocation, path: string): AppLocation {
  if (screenOf(path) === screenOf(location.path)) {
    return { path, params: { ...location.params } };
  }
  const params: Record<string, string> = {};
  for (const key of SCREEN_INDEPENDENT_PARAMS) {
    const value = location.params[key];
    if (value !== undefined) {
      params[key] = value;
    }
  }
  return { path, params };
}
