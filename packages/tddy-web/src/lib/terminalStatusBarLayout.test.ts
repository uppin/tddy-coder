import { describe, expect, it } from "bun:test";
import {
  controlCenterStrictlyInsideRect,
  plannedChromeCentersClearTerminalCanvas,
  statusBarBottomMeetsOrAboveTerminalTop,
  type ViewRect,
} from "./terminalStatusBarLayout";

describe("terminalStatusBarLayout (PRD geometry — Green implementation)", () => {
  it("controlCenterStrictlyInsideRect is true when inner center lies inside outer", () => {
    const outer: ViewRect = { left: 0, top: 0, right: 100, bottom: 100 };
    const inner: ViewRect = { left: 40, top: 40, right: 60, bottom: 60 };
    expect(controlCenterStrictlyInsideRect(inner, outer)).toBe(true);
  });

  it("statusBarBottomMeetsOrAboveTerminalTop is true when bar sits flush above terminal", () => {
    const bar: ViewRect = { left: 0, top: 0, right: 400, bottom: 32 };
    const term: ViewRect = { left: 0, top: 32, right: 400, bottom: 432 };
    expect(statusBarBottomMeetsOrAboveTerminalTop(bar, term)).toBe(true);
  });

  it("plannedChromeCentersClearTerminalCanvas is true when controls are above the canvas", () => {
    const terminal: ViewRect = { left: 10, top: 100, right: 500, bottom: 600 };
    const controlAbove: ViewRect = { left: 20, top: 40, right: 44, bottom: 64 };
    expect(plannedChromeCentersClearTerminalCanvas(terminal, [controlAbove])).toBe(true);
  });
});
