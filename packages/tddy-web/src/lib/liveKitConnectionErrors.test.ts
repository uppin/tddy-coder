import { describe, expect, it } from "bun:test";
import { ConnectionError } from "livekit-client";
import { isCancelledLiveKitConnectionError } from "./liveKitConnectionErrors";

describe("isCancelledLiveKitConnectionError", () => {
  it("returns true for SDK cancel used when disconnect aborts connect", () => {
    expect(
      isCancelledLiveKitConnectionError(
        ConnectionError.cancelled("Client initiated disconnect"),
      ),
    ).toBe(true);
  });

  it("returns false for other connection errors", () => {
    expect(
      isCancelledLiveKitConnectionError(
        ConnectionError.timeout("timed out"),
      ),
    ).toBe(false);
  });

  it("returns false for non-LiveKit errors", () => {
    expect(isCancelledLiveKitConnectionError(new Error("fail"))).toBe(false);
  });
});
