import { describe, expect, test } from "bun:test";
import {
  canPitchIn,
  canPitchOut,
  clampTerminalFontSize,
  DEFAULT_TERMINAL_FONT_MAX,
  DEFAULT_TERMINAL_FONT_MIN,
  pitchInFontSize,
  pitchOutFontSize,
  reduceTrackpadPinchAccum,
  TRACKPAD_PINCH_STEP_ACCUM_PX,
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

  test("reduceTrackpadPinchAccum resets when ctrlKey is false", () => {
    const r = reduceTrackpadPinchAccum(10, -5, false, TRACKPAD_PINCH_STEP_ACCUM_PX, 14, {});
    expect(r.accum).toBe(0);
    expect(r.fontSize).toBe(14);
  });

  test("reduceTrackpadPinchAccum pitch-in when deltaY negative and span exceeds step", () => {
    const r = reduceTrackpadPinchAccum(0, -120, true, TRACKPAD_PINCH_STEP_ACCUM_PX, 14, {});
    expect(r.fontSize).toBeGreaterThan(14);
    expect(r.accum).toBeGreaterThan(-TRACKPAD_PINCH_STEP_ACCUM_PX);
  });

  test("reduceTrackpadPinchAccum pitch-out when deltaY positive", () => {
    const r = reduceTrackpadPinchAccum(0, 120, true, TRACKPAD_PINCH_STEP_ACCUM_PX, 20, {});
    expect(r.fontSize).toBeLessThan(20);
  });
});
