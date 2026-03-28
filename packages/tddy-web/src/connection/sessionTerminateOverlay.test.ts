import { describe, expect, it } from "bun:test";
import { Signal } from "../gen/connection_pb";
import {
  buildTerminateOverlayAriaLabel,
  delegateSignalSessionRpc,
} from "./sessionTerminateOverlay";

describe("sessionTerminateOverlay (daemon SIGINT path)", () => {
  it("delegateSignalSessionRpc completes after delegating SignalSession", async () => {
    let called = false;
    await delegateSignalSessionRpc({
      sessionToken: "tok",
      sessionId: "sid",
      signal: Signal.SIGINT,
      signalSession: async () => {
        called = true;
      },
      trace: false,
    });
    expect(called).toBe(true);
  });

  it("buildTerminateOverlayAriaLabel describes daemon-backed SIGINT like Connection Screen", () => {
    expect(buildTerminateOverlayAriaLabel()).toMatch(/daemon/i);
    expect(buildTerminateOverlayAriaLabel().toLowerCase()).toContain("sigint");
  });
});
