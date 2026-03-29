import { describe, expect, test } from "bun:test";
import { confirmRemoteSessionTermination } from "./remoteTerminateConfirm";

describe("confirmRemoteSessionTermination", () => {
  test("delegates to window.confirm and returns its boolean result", () => {
    const g = globalThis as typeof globalThis & { window?: Window };
    const prev = g.window;
    let calledWith = "";
    g.window = {
      ...prev,
      confirm: (msg?: string) => {
        calledWith = String(msg ?? "");
        return false;
      },
    } as Window;
    try {
      expect(confirmRemoteSessionTermination("Stop remote?")).toBe(false);
      expect(calledWith).toContain("Stop remote?");
    } finally {
      if (prev !== undefined) g.window = prev;
      else delete g.window;
    }
  });
});
