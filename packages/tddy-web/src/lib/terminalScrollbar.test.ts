/**
 * Unit tests — `computeScrollbar` (native ghostty Scrollbar coordinate mapping).
 *
 * docs/ft/web/terminal-replay-lazy-scroll.md (amended — terminal-native-scrolling)
 *
 * Maps ghostty-web viewport coordinates into the native `PageList.Scrollbar {total, offset, len}`
 * coordinate space (the single source of truth for viewport position, same space as `scrollToLine`):
 *   total = scrollbackLength + rows
 *   offset = max(0, scrollbackLength - viewportY)   (absolute row of the first visible line; 0 = top)
 *   len = rows
 * These tests pin the mapping and its clamping at the boundaries.
 */

import { describe, expect, it } from "bun:test";
import { computeScrollbar } from "./terminalScrollbar";

describe("computeScrollbar", () => {
  it("at the bottom (viewportY 0) reports offset = scrollbackLength and total = scrollbackLength + rows", () => {
    // Given — 200 lines of scrollback, a 24-row viewport, viewport pinned to the bottom (viewportY 0)
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportY: 0, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 200, len: 24 });
  });

  it("at the top (viewportY = scrollbackLength) reports offset 0", () => {
    // Given — scrolled all the way up to the top of the scrollback
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportY: 200, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 0, len: 24 });
  });

  it("in the middle (viewportY = K) reports offset = scrollbackLength - K", () => {
    // Given — scrolled up 50 lines from the bottom
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportY: 50, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 150, len: 24 });
  });

  it("clamps viewportY above the scrollback top to offset 0 (never negative)", () => {
    // Given — viewportY overshoots the scrollback length (defensive clamp)
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportY: 999, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 0, len: 24 });
  });

  it("clamps negative viewportY to offset = scrollbackLength (never exceeds the bottom)", () => {
    // Given — a negative viewportY (should not happen, but must not produce offset > scrollbackLength)
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportY: -5, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 200, len: 24 });
  });

  it("with zero scrollback reports total = rows and offset 0 (no history to scroll through)", () => {
    // Given — no scrollback retained (e.g. the alternate buffer, or scrollback configured to 0)
    // When
    const sb = computeScrollbar({ scrollbackLength: 0, viewportY: 0, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 24, offset: 0, len: 24 });
  });
});
