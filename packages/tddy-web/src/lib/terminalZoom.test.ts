import { describe, expect, test } from "bun:test";
import {
  canPitchIn,
  canPitchOut,
  clampTerminalFontSize,
  DEFAULT_TERMINAL_FONT_MAX,
  DEFAULT_TERMINAL_FONT_MIN,
  pitchInFontSize,
  pitchOutFontSize,
} from "./terminalZoom";

describe("terminalZoom (granular — Green phase implements correct math)", () => {
  test("clampTerminalFontSize enforces min and max", () => {
    expect(clampTerminalFontSize(5)).toBe(DEFAULT_TERMINAL_FONT_MIN);
    expect(clampTerminalFontSize(100)).toBe(DEFAULT_TERMINAL_FONT_MAX);
    expect(clampTerminalFontSize(20)).toBe(20);
  });

  test("pitchInFontSize increases by step until max", () => {
    expect(pitchInFontSize(14)).toBe(15);
    expect(pitchInFontSize(31)).toBe(32);
  });

  test("pitchOutFontSize decreases by step until min", () => {
    expect(pitchOutFontSize(14)).toBe(13);
    expect(pitchOutFontSize(9)).toBe(8);
    expect(pitchOutFontSize(8)).toBe(8);
  });

  test("canPitchIn is false at max", () => {
    expect(canPitchIn(31)).toBe(true);
    expect(canPitchIn(32)).toBe(false);
  });

  test("canPitchOut is false at min", () => {
    expect(canPitchOut(9)).toBe(true);
    expect(canPitchOut(8)).toBe(false);
  });
});
