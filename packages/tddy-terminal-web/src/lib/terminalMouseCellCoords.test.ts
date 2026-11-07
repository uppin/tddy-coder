import { describe, expect, it } from "bun:test";
import { clientPointToTerminalCell } from "./terminalMouseCellCoords";

describe("clientPointToTerminalCell", () => {
  // 800×480 px canvas with 80×24 cells → 10×20 px per cell
  const grid80x24 = { left: 0, top: 0, width: 800, height: 480 };

  it("maps the top-left pixel to cell (1,1)", () => {
    // Given / When / Then
    expect(clientPointToTerminalCell(0, 0, grid80x24, 80, 24)).toEqual({ col: 1, row: 1 });
  });

  it("uses canvas width/height for cell size (repro: container wider than canvas would skew cols)", () => {
    // Given — Grid 800×480, 80×24 cells ⇒ 10×20 px per cell. x=395 → col 40; y=50 → row 3.
    // When / Then
    expect(clientPointToTerminalCell(395, 50, grid80x24, 80, 24)).toEqual({ col: 40, row: 3 });
  });

  it("clamps out-of-bounds low coordinates to cell (1,1)", () => {
    // Given / When / Then
    expect(clientPointToTerminalCell(-10, -5, grid80x24, 80, 24)).toEqual({ col: 1, row: 1 });
  });

  it("clamps out-of-bounds high coordinates to the last cell", () => {
    // Given / When / Then
    expect(clientPointToTerminalCell(900, 600, grid80x24, 80, 24)).toEqual({ col: 80, row: 24 });
  });

  it("returns null for zero column count", () => {
    // When / Then
    expect(clientPointToTerminalCell(0, 0, grid80x24, 0, 24)).toBeNull();
  });

  it("returns null for zero row count", () => {
    // When / Then
    expect(clientPointToTerminalCell(0, 0, grid80x24, 80, 0)).toBeNull();
  });

  it("returns null for a zero-width grid rect", () => {
    // When / Then
    expect(clientPointToTerminalCell(0, 0, { left: 0, top: 0, width: 0, height: 100 }, 80, 24)).toBeNull();
  });
});
