import { describe, expect, it } from "bun:test";
import {
  handleTerminateOverlayClick,
  notifyRemoteSessionEnded,
} from "./ghosttyTerminalSessionHandlers";

describe("ghosttyTerminalSessionHandlers", () => {
  it("handleTerminateOverlayClick forwards to onTerminate", () => {
    let calls = 0;
    handleTerminateOverlayClick(() => {
      calls += 1;
    });
    expect(calls).toBe(1);
  });

  it("notifyRemoteSessionEnded forwards to onRemoteSessionEnded", () => {
    let calls = 0;
    notifyRemoteSessionEnded(() => {
      calls += 1;
    });
    expect(calls).toBe(1);
  });
});
