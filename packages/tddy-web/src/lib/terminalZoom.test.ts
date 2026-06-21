import { describe, expect, it } from "bun:test";
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

describe("terminalZoom", () => {
  it("clamps the font size to the minimum when the input is below the floor", () => {
    // When
    const result = clampTerminalFontSize(5);
    // Then
    expect(result).toBe(DEFAULT_TERMINAL_FONT_MIN);
  });

  it("clamps the font size to the maximum when the input is above the ceiling", () => {
    // When
    const result = clampTerminalFontSize(100);
    // Then
    expect(result).toBe(DEFAULT_TERMINAL_FONT_MAX);
  });

  it("returns the value unchanged when it is within the allowed bounds", () => {
    // When
    const result = clampTerminalFontSize(20);
    // Then
    expect(result).toBe(20);
  });

  it("increases the font size by one step when pitching in below the maximum", () => {
    // When + Then
    expect(pitchInFontSize(14)).toBe(15);
    expect(pitchInFontSize(31)).toBe(32);
  });

  it("decreases the font size by one step when pitching out above the minimum", () => {
    // When + Then
    expect(pitchOutFontSize(14)).toBe(13);
    expect(pitchOutFontSize(9)).toBe(8);
  });

  it("does not decrease the font size below the minimum when pitching out at the floor", () => {
    // When
    const result = pitchOutFontSize(8);
    // Then
    expect(result).toBe(8);
  });

  it("allows pitch-in when the font size is below the maximum", () => {
    // When + Then
    expect(canPitchIn(31)).toBe(true);
  });

  it("blocks pitch-in when the font size is already at the maximum", () => {
    // When
    const result = canPitchIn(32);
    // Then
    expect(result).toBe(false);
  });

  it("allows pitch-out when the font size is above the minimum", () => {
    // When + Then
    expect(canPitchOut(9)).toBe(true);
  });

  it("blocks pitch-out when the font size is already at the minimum", () => {
    // When
    const result = canPitchOut(8);
    // Then
    expect(result).toBe(false);
  });

  it("resets the accumulator and leaves the font size unchanged when ctrlKey is false", () => {
    // When
    const result = reduceTrackpadPinchAccum(10, -5, false, TRACKPAD_PINCH_STEP_ACCUM_PX, 14, {});

    // Then
    expect(result.accum).toBe(0);
    expect(result.fontSize).toBe(14);
  });

  it("increases the font size and wraps the accumulator when a negative deltaY exceeds the pitch step", () => {
    // When
    const result = reduceTrackpadPinchAccum(0, -120, true, TRACKPAD_PINCH_STEP_ACCUM_PX, 14, {});

    // Then
    // Font size must be strictly larger — exact increment depends on step math
    expect(result.fontSize).toBeGreaterThan(14);
    // Accumulator wraps back within one step after the pitch fires
    expect(result.accum).toBeGreaterThan(-TRACKPAD_PINCH_STEP_ACCUM_PX);
  });

  it("decreases the font size when a positive deltaY exceeds the pitch step", () => {
    // When
    const result = reduceTrackpadPinchAccum(0, 120, true, TRACKPAD_PINCH_STEP_ACCUM_PX, 20, {});

    // Then
    // Font size must be strictly smaller — exact decrement depends on step math
    expect(result.fontSize).toBeLessThan(20);
  });
});
