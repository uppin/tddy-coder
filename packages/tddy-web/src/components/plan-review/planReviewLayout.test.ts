import { describe, expect, it } from "bun:test";
import { terminalAndPlanAreSideBySide } from "./planReviewLayout";

describe("terminalAndPlanAreSideBySide", () => {
  it("returns true when terminal is left column and plan is right column (non-overlapping)", () => {
    const term = { left: 0, right: 400, top: 0, bottom: 600 };
    const plan = { left: 400, right: 960, top: 0, bottom: 600 };
    expect(terminalAndPlanAreSideBySide(term, plan, 960, 600)).toBe(true);
  });

  it("returns false when columns overlap horizontally (not side-by-side)", () => {
    const term = { left: 0, right: 500, top: 0, bottom: 600 };
    const plan = { left: 400, right: 960, top: 0, bottom: 600 };
    expect(terminalAndPlanAreSideBySide(term, plan, 960, 600)).toBe(false);
  });

  it("returns false when terminal has zero height", () => {
    const term = { left: 0, right: 400, top: 100, bottom: 100 };
    const plan = { left: 400, right: 960, top: 0, bottom: 600 };
    expect(terminalAndPlanAreSideBySide(term, plan, 960, 600)).toBe(false);
  });

  it("returns false when vertical bands do not overlap", () => {
    const term = { left: 0, right: 400, top: 0, bottom: 200 };
    const plan = { left: 400, right: 960, top: 300, bottom: 600 };
    expect(terminalAndPlanAreSideBySide(term, plan, 960, 600)).toBe(false);
  });

  it("returns false when a pane extends past the viewport width", () => {
    const term = { left: 0, right: 400, top: 0, bottom: 600 };
    const plan = { left: 400, right: 1000, top: 0, bottom: 600 };
    expect(terminalAndPlanAreSideBySide(term, plan, 960, 600)).toBe(false);
  });
});
