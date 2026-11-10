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

/** Accumulated `wheel.deltaY` (trackpad pinch) before one font step. */
export const TRACKPAD_PINCH_STEP_ACCUM_PX = 48;

/**
 * Merge a wheel event into running pinch accumulation and compute the next font size.
 * Used for laptop trackpads (wheel + `ctrlKey`). Touchscreens use touch span instead.
 */
export function reduceTrackpadPinchAccum(
  accum: number,
  deltaY: number,
  ctrlKey: boolean,
  stepPx: number,
  startFont: number,
  opts: TerminalZoomStepOptions = {}
): { accum: number; fontSize: number } {
  if (!ctrlKey) {
    return { accum: 0, fontSize: startFont };
  }
  let a = accum + deltaY;
  let font = startFont;
  while (a <= -stepPx && canPitchIn(font, opts)) {
    font = pitchInFontSize(font, opts);
    a += stepPx;
  }
  while (a >= stepPx && canPitchOut(font, opts)) {
    font = pitchOutFontSize(font, opts);
    a -= stepPx;
  }
  return { accum: a, fontSize: font };
}
