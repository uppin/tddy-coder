/**
 * Where the sessions drawer sends an operator to set up a project's home agent: the Projects screen,
 * naming the project (`#/projects?project=<id>`), on the host they were already looking at.
 *
 * Pure (no DOM), like `appLocation`; the caller navigates to the result.
 */

import { PARAM_PROJECT, withParams, withPath, type AppLocation } from "./appLocation";
import { PROJECTS_ROUTE } from "./appRoutes";

/**
 * The Projects location for `projectId`. A screen change, so {@link withPath} keeps only the
 * screen-independent params (the selected host) and drops the Sessions screen's own selection.
 */
export function projectSetupLocation(location: AppLocation, projectId: string): AppLocation {
  return withParams(withPath(location, PROJECTS_ROUTE), { [PARAM_PROJECT]: projectId });
}
