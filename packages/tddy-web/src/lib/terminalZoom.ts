/**
 * Terminal font zoom (pitch in / out) — bounds and stepping.
 */

export const DEFAULT_TERMINAL_FONT_MIN = 8;
export const DEFAULT_TERMINAL_FONT_MAX = 32;
export const DEFAULT_TERMINAL_ZOOM_STEP = 1;

export interface TerminalZoomStepOptions {
  min?: number;
  max?: number;
  step?: number;
}

/** Clamp font size to [min, max]. */
export function clampTerminalFontSize(
  n: number,
  min: number = DEFAULT_TERMINAL_FONT_MIN,
  max: number = DEFAULT_TERMINAL_FONT_MAX
): number {
  return Math.min(max, Math.max(min, n));
}

/** Next font size after pitch-in (larger glyphs), clamped to bounds. */
export function pitchInFontSize(
  current: number,
  opts: TerminalZoomStepOptions = {}
): number {
  const min = opts.min ?? DEFAULT_TERMINAL_FONT_MIN;
  const max = opts.max ?? DEFAULT_TERMINAL_FONT_MAX;
  const step = opts.step ?? DEFAULT_TERMINAL_ZOOM_STEP;
  return clampTerminalFontSize(current + step, min, max);
}

/** Next font size after pitch-out (smaller glyphs), clamped to bounds. */
export function pitchOutFontSize(
  current: number,
  opts: TerminalZoomStepOptions = {}
): number {
  const min = opts.min ?? DEFAULT_TERMINAL_FONT_MIN;
  const max = opts.max ?? DEFAULT_TERMINAL_FONT_MAX;
  const step = opts.step ?? DEFAULT_TERMINAL_ZOOM_STEP;
  return clampTerminalFontSize(current - step, min, max);
}

/** Whether pitch-in can increase font within bounds. */
export function canPitchIn(
  current: number,
  opts: TerminalZoomStepOptions = {}
): boolean {
  const max = opts.max ?? DEFAULT_TERMINAL_FONT_MAX;
  const step = opts.step ?? DEFAULT_TERMINAL_ZOOM_STEP;
  return current + step <= max;
}

/** Whether pitch-out can decrease font within bounds. */
export function canPitchOut(
  current: number,
  opts: TerminalZoomStepOptions = {}
): boolean {
  const min = opts.min ?? DEFAULT_TERMINAL_FONT_MIN;
  const step = opts.step ?? DEFAULT_TERMINAL_ZOOM_STEP;
  return current - step >= min;
}
