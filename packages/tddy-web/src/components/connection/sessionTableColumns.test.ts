import { describe, expect, test } from "bun:test";
import {
  sessionTableRemovalBreakpointsPx,
  sessionTableResponsiveContainerCss,
  visibleSessionTableColumnKeysForLayout,
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

  test("container CSS mirrors column min-width policy (model @ max-width 399px)", () => {
    const css = sessionTableResponsiveContainerCss();
    expect(css).toContain("max-width: 399px");
    expect(css).toContain('[data-session-col="model"]');
  });

  test("visible column keys follow session table host width when it is narrower than the window", () => {
    const windowWidthPx = 1440;
    const sessionTableHostWidthPx = 360;
    const keys = visibleSessionTableColumnKeysForLayout(windowWidthPx, sessionTableHostWidthPx);
    expect(keys).toEqual(
      visibleSessionTableColumnKeysForViewportWidth(
        Math.min(windowWidthPx, sessionTableHostWidthPx),
      ),
    );
    expect(keys.includes("model")).toBe(false);
  });
});
