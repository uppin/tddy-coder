/**
 * Unit tests for the Agent Activity sidebar's default open state.
 *
 * Feature: docs/ft/web/agent-activity-pane.md § Right sidebar (`#project-home` 3/7).
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
