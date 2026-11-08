/**
 * Maps a top-based viewport position into the native `PageList.Scrollbar {total, offset, len}`
 * coordinate space (same space as `scrollToLine`: 0 = top of the scrollback).
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md (amended — terminal-native-scrolling),
 * docs/ft/web/1-WIP/PRD-2026-11-08-terminal-engine-interface.md (top-based scrolling)
 */

export interface Scrollbar {
  total: number;
  offset: number;
  len: number;
}

export function computeScrollbar(args: {
  scrollbackLength: number;
  /** The buffer line on the viewport's first row, counted from the top (0 = oldest line). */
  viewportTop: number;
  rows: number;
}): Scrollbar {
  const { scrollbackLength, viewportTop, rows } = args;
  const total = scrollbackLength + rows;
  const offset = Math.max(0, Math.min(scrollbackLength, viewportTop));
  return { total, offset, len: rows };
}
