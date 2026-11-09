/**
 * The route grammar a component package needs to link into screens it does not own — pure string rules, no DOM, no React.
 * The rest of the grammar lives with the app (`tddy-web` `routing/appRoutes.ts`).
 */

/**
 * The single path segment following `prefix`, or `null` when `pathname` is not
 * `{prefix}/{one-segment}`. Raw (still percent-encoded) so an encoded `/` inside a segment cannot
 * be mistaken for a separator.
 */
export function singleSegmentAfter(pathname: string, prefix: string): string | null {
  const withSlash = `${prefix}/`;
  if (!pathname.startsWith(withSlash)) {
    return null;
  }
  const segment = pathname.slice(withSlash.length);
  return segment === "" || segment.includes("/") ? null : segment;
}

/** Percent-decode a raw path segment, treating a malformed escape as "no match". */
export function decodeSegment(segment: string | null): string | null {
  if (segment === null) {
    return null;
  }
  try {
    return decodeURIComponent(segment);
  } catch {
    return null;
  }
}

/** Path for the sessions drawer screen and its deep links. */
export const SESSIONS_DRAWER_ROUTE = "/sessions";

/**
 * Prefix of the pane that sets up a project's **main** session: `/sessions/project-home/:projectId`,
 * where a click on the drawer's pinned **main** entry lands when the project has no main, and
 * `/sessions/project-home/:projectId/replace`, where its **Replace main** action lands. Two or three
 * segments, so it can never be read as a session id.
 */
export const SESSIONS_PROJECT_HOME_SETUP_ROUTE = `${SESSIONS_DRAWER_ROUTE}/project-home`;

/** The trailing segment that opens the setup pane in replace mode. */
export const PROJECT_HOME_REPLACE_SEGMENT = "replace";

/**
 * Builds the `/sessions/project-home/:projectId` path for the given project id, or its
 * `/replace` form when `replacing`.
 */
export function sessionsProjectHomeSetupPath(
  projectId: string,
  options: { replacing?: boolean } = {},
): string {
  const path = `${SESSIONS_PROJECT_HOME_SETUP_ROUTE}/${encodeURIComponent(projectId)}`;
  return options.replacing ? `${path}/${PROJECT_HOME_REPLACE_SEGMENT}` : path;
}

/** Builds a `/sessions/:id` deep-link path for the given session id. */
export function sessionsDrawerPathForSession(sessionId: string): string {
  return `${SESSIONS_DRAWER_ROUTE}/${encodeURIComponent(sessionId)}`;
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
