import { describe, expect, it, mock } from "bun:test";
import { requestElementFullscreen } from "./browserFullscreen";

describe("requestElementFullscreen", () => {
  it("delegates the fullscreen request to Element.requestFullscreen on the target element", async () => {
    // Given
    const requestFullscreen = mock(() => Promise.resolve());
    const el = { requestFullscreen } as unknown as Element;

    // When
    await requestElementFullscreen(el);

    // Then
    expect(requestFullscreen).toHaveBeenCalled();
  });
});
