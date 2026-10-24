/**
 * Unit tests for which session types can run a project's main, and the reason each other type
 * gives in the setup form.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import { describe, expect, it } from "bun:test";
import { whyMainCannotRun } from "./projectHomeSessionTypes";

describe("whyMainCannotRun", () => {
  it("lets Claude CLI run main", () => {
    expect(whyMainCannotRun("claude-cli")).toBeNull();
  });

  it("explains that Cursor CLI has no tool allowlist", () => {
    expect(whyMainCannotRun("cursor-cli")).toBe(
      "Cursor CLI has no tool allowlist, so it could still use its own file tools and main would not stay read-only.",
    );
  });

  it("explains that a Tool session cannot run in managed-codebase mode", () => {
    expect(whyMainCannotRun("tool")).toBe(
      "Tool sessions cannot run in managed-codebase mode, so main would not stay read-only.",
    );
  });

  it("explains that OpenCode cannot run in managed-codebase mode", () => {
    expect(whyMainCannotRun("opencode-cli")).toBe(
      "OpenCode cannot run in managed-codebase mode, so main would not stay read-only.",
    );
  });

  it("explains that Muse Code cannot run in managed-codebase mode", () => {
    expect(whyMainCannotRun("muse-cli")).toBe(
      "Muse Code cannot run in managed-codebase mode, so main would not stay read-only.",
    );
  });

  it("explains that Warp cannot run in managed-codebase mode", () => {
    expect(whyMainCannotRun("warp-cli")).toBe(
      "Warp cannot run in managed-codebase mode, so main would not stay read-only.",
    );
  });
});
