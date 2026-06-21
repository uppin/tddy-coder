import { describe, expect, it } from "bun:test";
import { aViewRect } from "../test-utils";
import {
  controlCenterStrictlyInsideRect,
  plannedChromeCentersClearTerminalCanvas,
  statusBarBottomMeetsOrAboveTerminalTop,
} from "./terminalStatusBarLayout";

describe("terminalStatusBarLayout (PRD geometry — Green implementation)", () => {
  it("controlCenterStrictlyInsideRect is true when inner center lies inside outer", () => {
    // Given
    const outer = aViewRect({ left: 0, top: 0, right: 100, bottom: 100 });
    const inner = aViewRect({ left: 40, top: 40, right: 60, bottom: 60 });

    // When / Then
    expect(controlCenterStrictlyInsideRect(inner, outer)).toBe(true);
  });

  it("statusBarBottomMeetsOrAboveTerminalTop is true when bar sits flush above terminal", () => {
    // Given
    const bar = aViewRect({ left: 0, top: 0, right: 400, bottom: 32 });
    const term = aViewRect({ left: 0, top: 32, right: 400, bottom: 432 });

    // When / Then
    expect(statusBarBottomMeetsOrAboveTerminalTop(bar, term)).toBe(true);
  });

  it("plannedChromeCentersClearTerminalCanvas is true when controls are above the canvas", () => {
    // Given
    const terminal = aViewRect({ left: 10, top: 100, right: 500, bottom: 600 });
    const controlAbove = aViewRect({ left: 20, top: 40, right: 44, bottom: 64 });

    // When / Then
    expect(plannedChromeCentersClearTerminalCanvas(terminal, [controlAbove])).toBe(true);
  });
});
