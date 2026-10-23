/**
 * Where the drawer sends an operator to set up a project's home agent: the Projects screen, naming the
 * project, on the host they were already looking at.
 */

import { describe, it, expect } from "bun:test";
import { PARAM_FULL, PARAM_HOST, PARAM_INSPECTOR, PARAM_PROJECT } from "./appLocation";
import { PROJECTS_ROUTE, SESSIONS_DRAWER_ROUTE } from "./appRoutes";
import { projectSetupLocation } from "./projectSetupLocation";

describe("projectSetupLocation", () => {
  it("names the project on the Projects screen", () => {
    // Given
    const current = { path: SESSIONS_DRAWER_ROUTE, params: {} };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next).toEqual({ path: PROJECTS_ROUTE, params: { [PARAM_PROJECT]: "proj-beta" } });
  });

  it("keeps the selected host", () => {
    // Given
    const current = { path: SESSIONS_DRAWER_ROUTE, params: { [PARAM_HOST]: "mac" } };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next.params).toEqual({ [PARAM_HOST]: "mac", [PARAM_PROJECT]: "proj-beta" });
  });

  it("drops the selected session's inspector state, which names nothing on Projects", () => {
    // Given
    const current = {
      path: `${SESSIONS_DRAWER_ROUTE}/s1`,
      params: { [PARAM_HOST]: "mac", [PARAM_INSPECTOR]: "files", [PARAM_FULL]: "1" },
    };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next.params).toEqual({ [PARAM_HOST]: "mac", [PARAM_PROJECT]: "proj-beta" });
  });
});
