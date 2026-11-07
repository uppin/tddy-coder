import { describe, expect, it } from "bun:test";
import { confirmRemoteSessionTermination } from "./remoteTerminateConfirm";

/** Temporarily replaces window.confirm with a spy, runs fn, then restores the original. */
function withWindowConfirmSpy(
  returnValue: boolean,
  fn: (spy: { calledWith: string }) => void,
): void {
  const g = globalThis as typeof globalThis & { window?: Window };
  const prev = g.window;
  const spy = { calledWith: "" };
  g.window = {
    ...prev,
    confirm: (msg?: string) => {
      spy.calledWith = String(msg ?? "");
      return returnValue;
    },
  } as Window;
  try {
    fn(spy);
  } finally {
    if (prev !== undefined) g.window = prev;
    else delete g.window;
  }
}

describe("confirmRemoteSessionTermination", () => {
  it("returns false when window.confirm returns false", () => {
    // Given — confirm is stubbed to return false
    withWindowConfirmSpy(false, () => {
      // When / Then
      expect(confirmRemoteSessionTermination("Stop remote?")).toBe(false);
    });
  });

  it("forwards the message string to window.confirm", () => {
    // Given — confirm is stubbed; we only care about what it was called with
    withWindowConfirmSpy(false, (spy) => {
      // When
      confirmRemoteSessionTermination("Stop remote?");

      // Then
      expect(spy.calledWith).toContain("Stop remote?");
    });
  });
});
