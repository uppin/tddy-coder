/**
 * Unit tests for the Agent Activity sidebar's default open state.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-activity-sidebar.md (`#project-home` 3/7).
 */

import { describe, it, expect } from "bun:test";
import { defaultActivitySidebarOpen } from "./activitySidebarState";

describe("defaultActivitySidebarOpen", () => {
  it("defaults the sidebar open for a managed session", () => {
    expect(defaultActivitySidebarOpen({ managedCodebase: true })).toBe(true);
  });

  it("defaults the sidebar closed for an unmanaged session", () => {
    expect(defaultActivitySidebarOpen({ managedCodebase: false })).toBe(false);
  });
});
