import { describe, expect, test } from "bun:test";
import { clientPointToTerminalCell } from "./terminalMouseCellCoords";

describe("clientPointToTerminalCell", () => {
  const grid80x24 = { left: 0, top: 0, width: 800, height: 480 };

  test("maps top-left pixel to cell (1,1)", () => {
    expect(clientPointToTerminalCell(0, 0, grid80x24, 80, 24)).toEqual({ col: 1, row: 1 });
  });

  test("uses canvas width/height for cell size (repro: container wider than canvas would skew cols)", () => {
    // Grid 800×480, 80×24 cells => 10×20 px per cell. x=395 → col 40; y=50 → row 3.
    expect(clientPointToTerminalCell(395, 50, grid80x24, 80, 24)).toEqual({ col: 40, row: 3 });
  });

  test("clamps to grid bounds", () => {
    expect(clientPointToTerminalCell(-10, -5, grid80x24, 80, 24)).toEqual({ col: 1, row: 1 });
    expect(clientPointToTerminalCell(900, 600, grid80x24, 80, 24)).toEqual({ col: 80, row: 24 });
  });

  test("returns null for non-positive cols or rows", () => {
    expect(clientPointToTerminalCell(0, 0, grid80x24, 0, 24)).toBeNull();
    expect(clientPointToTerminalCell(0, 0, grid80x24, 80, 0)).toBeNull();
  });

  test("returns null for zero-sized grid rect", () => {
    expect(clientPointToTerminalCell(0, 0, { left: 0, top: 0, width: 0, height: 100 }, 80, 24)).toBeNull();
  });
});
