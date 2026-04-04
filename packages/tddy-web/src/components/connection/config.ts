/**
 * Floating terminal overlay — edit values here to tune default size, grid, and resize limits.
 *
 * Used by `terminalPresentation.ts` (re-exported) and overlay UI. Width is derived from height and
 * the logical grid unless capped by `TERMINAL_OVERLAY_PANE_MAX_DEFAULT_WIDTH_PX`.
 */

/** Logical terminal size (cells) for the compact / overlay pane. */
export const TERMINAL_OVERLAY_COLS = 80;
export const TERMINAL_OVERLAY_ROWS = 24;

/** Default height of the terminal region in pixels (how tall the pane opens). */
export const TERMINAL_OVERLAY_PANE_HEIGHT_PX = 180;

/** Maximum default width in pixels (prevents the pane from growing only sideways). */
export const TERMINAL_OVERLAY_PANE_MAX_DEFAULT_WIDTH_PX = 320;

/** Default width: at most the cap, otherwise enough px for `cols`×`rows` at the chosen height. */
export const TERMINAL_OVERLAY_PANE_WIDTH_PX = Math.min(
  TERMINAL_OVERLAY_PANE_MAX_DEFAULT_WIDTH_PX,
  Math.round((TERMINAL_OVERLAY_PANE_HEIGHT_PX * TERMINAL_OVERLAY_COLS) / TERMINAL_OVERLAY_ROWS),
);

/** Minimum font size (px) when scaling the fixed overlay grid. */
export const TERMINAL_OVERLAY_FONT_MIN_PX = 2;

/**
 * Approximate ratio of **cell width** to **font size** for the embedded monospace renderer (X-axis fit).
 * Used with `paneWidth / (cols * ratio)` so font tracks pane width on resize. Tune if glyphs look clipped.
 */
export const TERMINAL_OVERLAY_FIXED_GRID_CHAR_WIDTH_EM = 0.62;

/** Drag-resize lower bounds (floating pane; terminal grid area). */
export const TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX = 160;
export const TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX = 48;
