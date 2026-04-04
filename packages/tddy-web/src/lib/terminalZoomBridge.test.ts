import { describe, expect, test } from "bun:test";
import {
  parseTerminalFontSizeSyncDetail,
  parseTerminalZoomBridgeDetail,
} from "./terminalZoomBridge";

describe("terminalZoomBridge parsers", () => {
  test("parseTerminalZoomBridgeDetail accepts valid pitch-in", () => {
    const d = parseTerminalZoomBridgeDetail({
      action: "pitch-in",
      baselineFontSize: 14,
      opts: { min: 8, max: 32 },
    });
    expect(d).not.toBeNull();
    expect(d?.action).toBe("pitch-in");
    expect(d?.baselineFontSize).toBe(14);
    expect(d?.opts?.min).toBe(8);
    expect(d?.opts?.max).toBe(32);
  });

  test("parseTerminalZoomBridgeDetail rejects invalid action", () => {
    expect(
      parseTerminalZoomBridgeDetail({
        action: "zoom",
        baselineFontSize: 14,
      })
    ).toBeNull();
  });

  test("parseTerminalZoomBridgeDetail rejects non-finite baseline", () => {
    expect(
      parseTerminalZoomBridgeDetail({
        action: "reset",
        baselineFontSize: NaN,
      })
    ).toBeNull();
  });

  test("parseTerminalFontSizeSyncDetail accepts valid detail", () => {
    expect(parseTerminalFontSizeSyncDetail({ fontSize: 16 })).toBe(16);
  });

  test("parseTerminalFontSizeSyncDetail rejects bad fontSize", () => {
    expect(parseTerminalFontSizeSyncDetail({ fontSize: NaN })).toBeNull();
    expect(parseTerminalFontSizeSyncDetail(null)).toBeNull();
  });
});
