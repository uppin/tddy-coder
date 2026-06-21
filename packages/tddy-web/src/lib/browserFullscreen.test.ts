import { describe, expect, it, mock } from "bun:test";
import { requestFullscreenForConnectedTerminal } from "./browserFullscreen";

describe("requestFullscreenForConnectedTerminal", () => {
  it("delegates the fullscreen request to Element.requestFullscreen on the target element", async () => {
    // Given
    const requestFullscreen = mock(() => Promise.resolve());
    const el = { requestFullscreen } as unknown as Element;

    // When
    await requestFullscreenForConnectedTerminal(el);

    // Then
    expect(requestFullscreen).toHaveBeenCalled();
  });
});
