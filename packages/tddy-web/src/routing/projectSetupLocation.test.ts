/**
 * Where the drawer sends an operator to set up a project's home agent: the Projects screen, naming the
 * project, on the host they were already looking at.
 */

import { describe, it, expect } from "bun:test";
import { PARAM_HOST, PARAM_PROJECT } from "./appLocation";
import { PROJECTS_ROUTE } from "./appRoutes";
import { projectSetupLocation } from "./projectSetupLocation";

describe("projectSetupLocation", () => {
  it("names the project on the Projects screen", () => {
    // Given
    const current = { path: "/sessions", params: {} };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next).toEqual({ path: PROJECTS_ROUTE, params: { [PARAM_PROJECT]: "proj-beta" } });
  });

  it("keeps the selected host", () => {
    // Given
    const current = { path: "/sessions", params: { [PARAM_HOST]: "mac" } };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next.params).toEqual({ [PARAM_HOST]: "mac", [PARAM_PROJECT]: "proj-beta" });
  });

  it("drops the sessions screen's own selection, which names nothing on Projects", () => {
    // Given
    const current = { path: "/sessions", params: { [PARAM_HOST]: "mac", session: "s1", inspector: "files" } };

    // When
    const next = projectSetupLocation(current, "proj-beta");

    // Then
    expect(next.params).toEqual({ [PARAM_HOST]: "mac", [PARAM_PROJECT]: "proj-beta" });
  });
});
