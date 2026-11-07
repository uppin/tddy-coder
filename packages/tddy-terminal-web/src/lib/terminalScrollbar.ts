/**
 * Maps ghostty-web viewport coordinates into the native `PageList.Scrollbar {total, offset, len}`
 * coordinate space (same space as `scrollToLine`).
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md (amended — terminal-native-scrolling)
 */

export interface Scrollbar {
  total: number;
  offset: number;
  len: number;
}

export function computeScrollbar(args: {
  scrollbackLength: number;
  viewportY: number;
  rows: number;
}): Scrollbar {
  const { scrollbackLength, viewportY, rows } = args;
  const total = scrollbackLength + rows;
  const rawOffset = scrollbackLength - viewportY;
  const offset = Math.max(0, Math.min(scrollbackLength, rawOffset));
  return { total, offset, len: rows };
}
