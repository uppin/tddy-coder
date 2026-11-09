/**
 * Unit tests — `computeScrollbar` (native Scrollbar coordinate mapping, top-based).
 *
 * docs/ft/web/terminal-replay-lazy-scroll.md (amended — terminal-native-scrolling)
 * packages/tddy-terminal-web/docs/terminal-engine.md (top-based scrolling)
 *
 * The viewport position comes in counted from the top of the buffer (`viewportTop`: 0 = oldest
 * line, `scrollbackLength` = live tip) and maps onto `PageList.Scrollbar {total, offset, len}`:
 *   total  = scrollbackLength + rows
 *   offset = viewportTop, clamped to [0, scrollbackLength]
 *   len    = rows
 */

import { describe, expect, it } from "bun:test";
import { computeScrollbar } from "./terminalScrollbar";

describe("computeScrollbar", () => {
  it("at the live tip (viewportTop = scrollbackLength) reports offset = scrollbackLength and total = scrollbackLength + rows", () => {
    // Given — 200 lines of scrollback, a 24-row viewport following the latest output
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportTop: 200, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 200, len: 24 });
  });

  it("at the top (viewportTop 0) reports offset 0", () => {
    // Given — scrolled all the way up to the oldest retained line
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportTop: 0, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 0, len: 24 });
  });

  it("in the middle reports the viewport's top line as the offset", () => {
    // Given — the viewport's first row shows buffer line 150
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportTop: 150, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 150, len: 24 });
  });

  it("clamps a viewportTop past the live tip to offset = scrollbackLength", () => {
    // Given — viewportTop overshoots the scrollback (defensive clamp)
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportTop: 999, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 200, len: 24 });
  });

  it("clamps a negative viewportTop to offset 0 (never above the top)", () => {
    // Given — a negative viewportTop (should not happen, but must not produce a negative offset)
    // When
    const sb = computeScrollbar({ scrollbackLength: 200, viewportTop: -5, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 224, offset: 0, len: 24 });
  });

  it("with zero scrollback reports total = rows and offset 0 (no history to scroll through)", () => {
    // Given — no scrollback retained (e.g. the alternate buffer, or scrollback configured to 0)
    // When
    const sb = computeScrollbar({ scrollbackLength: 0, viewportTop: 0, rows: 24 });
    // Then
    expect(sb).toEqual({ total: 24, offset: 0, len: 24 });
  });
});
