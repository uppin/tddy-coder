import { describe, expect, it } from "bun:test";
import {
  parseTerminalFontSizeSyncDetail,
  parseTerminalZoomBridgeDetail,
} from "./terminalZoomBridge";

describe("terminalZoomBridge parsers", () => {
  it("parses a valid pitch-in detail and exposes action, baselineFontSize, and opts", () => {
    // Given
    const input = {
      action: "pitch-in",
      baselineFontSize: 14,
      opts: { min: 8, max: 32 },
    };

    // When
    const result = parseTerminalZoomBridgeDetail(input);

    // Then
    expect(result).not.toBeNull();
    expect(result?.action).toBe("pitch-in");
    expect(result?.baselineFontSize).toBe(14);
    expect(result?.opts?.min).toBe(8);
    expect(result?.opts?.max).toBe(32);
  });

  it("returns null for an unrecognised action string", () => {
    // When
    const result = parseTerminalZoomBridgeDetail({
      action: "zoom",
      baselineFontSize: 14,
    });
    // Then
    expect(result).toBeNull();
  });

  it("returns null when the baselineFontSize is not a finite number", () => {
    // When
    const result = parseTerminalZoomBridgeDetail({
      action: "reset",
      baselineFontSize: NaN,
    });
    // Then
    expect(result).toBeNull();
  });

  it("parses a valid font-size sync detail and returns the numeric font size", () => {
    // When
    const result = parseTerminalFontSizeSyncDetail({ fontSize: 16 });
    // Then
    expect(result).toBe(16);
  });

  it("returns null for a font-size sync detail with a non-finite fontSize", () => {
    // When + Then
    expect(parseTerminalFontSizeSyncDetail({ fontSize: NaN })).toBeNull();
    expect(parseTerminalFontSizeSyncDetail(null)).toBeNull();
  });
});
