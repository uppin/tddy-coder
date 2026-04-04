import { describe, expect, it } from "bun:test";
import {
  applyDedicatedTerminalBackToMini,
  applyOverlayPreviewClickToFull,
  attachKindForSessionControl,
  clampTerminalOverlayPaneSize,
  defaultTerminalMiniOverlayPlacement,
  nextPresentationFromAttach,
  reconcileReconnectOverlayInstances,
  TERMINAL_OVERLAY_COLS,
  TERMINAL_OVERLAY_PANE_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX,
  TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX,
  TERMINAL_OVERLAY_PANE_WIDTH_PX,
  TERMINAL_OVERLAY_ROWS,
} from "./terminalPresentation";

describe("overlay pane: default size & fixed 80×24 font scaling (header excluded)", () => {
  it("uses explicit default height and capped width (taller pane without matching width growth)", () => {
    expect(TERMINAL_OVERLAY_COLS).toBe(80);
    expect(TERMINAL_OVERLAY_ROWS).toBe(24);
    expect(TERMINAL_OVERLAY_PANE_WIDTH_PX).toBe(320);
    expect(TERMINAL_OVERLAY_PANE_HEIGHT_PX).toBe(180);
    expect(TERMINAL_OVERLAY_PANE_WIDTH_PX / TERMINAL_OVERLAY_PANE_HEIGHT_PX).not.toBeCloseTo(80 / 24, 5);
  });
});

describe("clampTerminalOverlayPaneSize", () => {
  it("clamps to min and max", () => {
    const a = clampTerminalOverlayPaneSize(50, 20, 800, 600);
    expect(a.width).toBe(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX);
    expect(a.height).toBe(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX);
    const b = clampTerminalOverlayPaneSize(2000, 2000, 400, 300);
    expect(b.width).toBe(400);
    expect(b.height).toBe(300);
  });

  it("when max is below min width, effective max is at least min", () => {
    const c = clampTerminalOverlayPaneSize(100, 40, 100, 40);
    expect(c.width).toBe(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX);
    expect(c.height).toBe(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX);
  });
});

describe("acceptance: presentation state — new session opens overlay, reconnect selects overlay", () => {
  it("new attach → overlay and no push to /terminal/:id (Connect opens pane, not dedicated screen)", () => {
    const r = nextPresentationFromAttach("hidden", "new");
    expect(r.presentation).toBe("overlay");
    expect(r.shouldPushTerminalRoute).toBe(false);
  });

  it("new attach from any prior presentation still uses overlay without route push", () => {
    const r = nextPresentationFromAttach("full", "new");
    expect(r.presentation).toBe("overlay");
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
