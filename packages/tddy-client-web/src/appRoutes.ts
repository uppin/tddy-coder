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

