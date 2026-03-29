import { describe, expect, test } from "bun:test";
import { shouldShowVisibleLiveKitStatusStrip } from "./liveKitStatusPresentation";

describe("shouldShowVisibleLiveKitStatusStrip", () => {
  test("hides plain status strip when connection overlay is on and status is connected", () => {
    expect(
      shouldShowVisibleLiveKitStatusStrip({
        connectionOverlayEnabled: true,
        status: "connected",
      }),
    ).toBe(false);
  });

  test("hides plain status strip when connection overlay is on and status is connecting", () => {
    expect(
      shouldShowVisibleLiveKitStatusStrip({
        connectionOverlayEnabled: true,
        status: "connecting",
      }),
    ).toBe(false);
  });
});
