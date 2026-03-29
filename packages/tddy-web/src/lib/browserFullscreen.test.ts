import { describe, expect, mock, test } from "bun:test";
import { requestFullscreenForConnectedTerminal } from "./browserFullscreen";

describe("requestFullscreenForConnectedTerminal", () => {
  test("calls Element.requestFullscreen on the target when present", async () => {
    const requestFullscreen = mock(() => Promise.resolve());
    const el = { requestFullscreen } as unknown as Element;
    await requestFullscreenForConnectedTerminal(el);
    expect(requestFullscreen).toHaveBeenCalled();
  });
});
