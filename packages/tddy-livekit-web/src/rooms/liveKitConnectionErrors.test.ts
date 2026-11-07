import { describe, expect, it } from "bun:test";
import { ConnectionError } from "livekit-client";
import { isCancelledLiveKitConnectionError } from "./liveKitConnectionErrors";

describe("isCancelledLiveKitConnectionError", () => {
  it("returns true for the SDK cancel error used when a client-initiated disconnect aborts a connect", () => {
    // Given
    const cancelled = ConnectionError.cancelled("Client initiated disconnect");

    // When / Then
    expect(isCancelledLiveKitConnectionError(cancelled)).toBe(true);
  });

  it("returns false for other LiveKit connection errors (e.g. timeout)", () => {
    // Given
    const timeout = ConnectionError.timeout("timed out");

    // When / Then
    expect(isCancelledLiveKitConnectionError(timeout)).toBe(false);
  });

  it("returns false for plain non-LiveKit errors", () => {
    // Given
    const plain = new Error("fail");

    // When / Then
    expect(isCancelledLiveKitConnectionError(plain)).toBe(false);
  });
});
