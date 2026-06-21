import { describe, expect, it } from "bun:test";
import { shouldShowVisibleLiveKitStatusStrip } from "./liveKitStatusPresentation";

describe("shouldShowVisibleLiveKitStatusStrip", () => {
  it("hides the plain status strip when the connection overlay is enabled and the state is connected", () => {
    // When
    const result = shouldShowVisibleLiveKitStatusStrip({
      connectionOverlayEnabled: true,
      status: "connected",
    });
    // Then
    expect(result).toBe(false);
  });

  it("hides the plain status strip when the connection overlay is enabled and the state is connecting", () => {
    // When
    const result = shouldShowVisibleLiveKitStatusStrip({
      connectionOverlayEnabled: true,
      status: "connecting",
    });
    // Then
    expect(result).toBe(false);
  });
});
