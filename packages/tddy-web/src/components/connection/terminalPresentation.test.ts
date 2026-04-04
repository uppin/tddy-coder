import { describe, expect, it } from "bun:test";
import {
  applyDedicatedTerminalBackToMini,
  applyOverlayPreviewClickToFull,
  attachKindForSessionControl,
  defaultTerminalMiniOverlayPlacement,
  nextPresentationFromAttach,
  reconcileReconnectOverlayInstances,
} from "./terminalPresentation";

describe("acceptance: presentation state — new session selects full, reconnect selects overlay", () => {
  it("new attach → full presentation and push to terminal route", () => {
    const r = nextPresentationFromAttach("hidden", "new");
    expect(r.presentation).toBe("full");
    expect(r.shouldPushTerminalRoute).toBe(true);
  });

  it("new attach while already full does not request redundant route push", () => {
    const r = nextPresentationFromAttach("full", "new");
    expect(r.presentation).toBe("full");
    expect(r.shouldPushTerminalRoute).toBe(false);
  });

  it("reconnect attach → overlay and no automatic push to /terminal/:id", () => {
    const r = nextPresentationFromAttach("hidden", "reconnect");
    expect(r.presentation).toBe("overlay");
    expect(r.shouldPushTerminalRoute).toBe(false);
  });
});

describe("acceptance: overlay click navigates to terminal route without second connect", () => {
  it("does not increment connect counters when expanding preview to full", () => {
    const before = { connectSessionCalls: 2, resumeSessionCalls: 1, disconnectCalls: 0 };
    const { presentation, counters } = applyOverlayPreviewClickToFull(before);
    expect(presentation).toBe("full");
    expect(counters.connectSessionCalls).toBe(before.connectSessionCalls);
    expect(counters.resumeSessionCalls).toBe(before.resumeSessionCalls);
    expect(counters.disconnectCalls).toBe(before.disconnectCalls);
  });
});

describe("acceptance: Back minimizes to mini without disconnect", () => {
  it("sets mini presentation and leaves disconnect count unchanged", () => {
    const before = { connectSessionCalls: 1, resumeSessionCalls: 0, disconnectCalls: 0 };
    const { presentation, counters } = applyDedicatedTerminalBackToMini(before);
    expect(presentation).toBe("mini");
    expect(counters.disconnectCalls).toBe(0);
    expect(counters.connectSessionCalls).toBe(before.connectSessionCalls);
  });
});

describe("acceptance: reconnect overlay idempotent", () => {
  it("multiple reconnect signals yield a single logical overlay instance", () => {
    expect(reconcileReconnectOverlayInstances(1)).toBe(1);
    expect(reconcileReconnectOverlayInstances(3)).toBe(1);
    expect(reconcileReconnectOverlayInstances(100)).toBe(1);
  });
});

describe("acceptance: session control → attach kind (for ConnectionScreen branching)", () => {
  it("startSession and connectSession are new attach; resumeSession is reconnect", () => {
    expect(attachKindForSessionControl("startSession")).toBe("new");
    expect(attachKindForSessionControl("connectSession")).toBe("new");
    expect(attachKindForSessionControl("resumeSession")).toBe("reconnect");
  });
});

describe("acceptance: default mini/overlay placement", () => {
  it("defaults to floating bottom-right", () => {
    expect(defaultTerminalMiniOverlayPlacement()).toBe("bottom-right");
  });
});

describe("unit: presentation transitions (granular)", () => {
  it("reconnect attach while already in overlay stays overlay without route push", () => {
    const r = nextPresentationFromAttach("overlay", "reconnect");
    expect(r.presentation).toBe("overlay");
    expect(r.shouldPushTerminalRoute).toBe(false);
  });
});
