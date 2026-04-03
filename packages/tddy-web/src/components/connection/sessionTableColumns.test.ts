import { describe, expect, test } from "bun:test";
import {
  sessionTableRemovalBreakpointsPx,
  visibleSessionTableColumnKeysForViewportWidth,
} from "./sessionTableColumns";

describe("sessionTableColumns (responsive column visibility policy)", () => {
  test("narrow viewport (375px) excludes model and agent from visible column keys", () => {
    const keys = visibleSessionTableColumnKeysForViewportWidth(375);
    expect(keys.includes("model")).toBe(false);
    expect(keys.includes("agent")).toBe(false);
  });

  test("removal breakpoints must be defined for tiered responsive hiding", () => {
    expect(sessionTableRemovalBreakpointsPx().length).toBeGreaterThan(0);
  });
});
