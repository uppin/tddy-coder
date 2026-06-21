import { describe, expect, it } from "bun:test";
import { aConnectCounter } from "../../test-utils";
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

// ---------------------------------------------------------------------------
// Default overlay size constants
// ---------------------------------------------------------------------------

describe("overlay pane: default size & 80×24 grid", () => {
  it("terminal overlay is 80 columns wide", () => {
    // Then
    expect(TERMINAL_OVERLAY_COLS).toBe(80);
  });

  it("terminal overlay is 24 rows tall", () => {
    // Then
    expect(TERMINAL_OVERLAY_ROWS).toBe(24);
  });

  it("default pane width is 320px", () => {
    // Then
    expect(TERMINAL_OVERLAY_PANE_WIDTH_PX).toBe(320);
  });

  it("default pane height is 180px", () => {
    // Then
    expect(TERMINAL_OVERLAY_PANE_HEIGHT_PX).toBe(180);
  });

  it("pane aspect ratio does not match the 80/24 grid ratio (taller pane, width does not scale with rows)", () => {
    // The overlay pane is intentionally taller relative to width than the pure 80/24 cell ratio.
    // not.toBeCloseTo is appropriate here: we're asserting the ratio is far from 80/24 (≈3.33),
    // not that it equals a specific value.
    expect(TERMINAL_OVERLAY_PANE_WIDTH_PX / TERMINAL_OVERLAY_PANE_HEIGHT_PX).not.toBeCloseTo(
      80 / 24,
      5,
    );
  });
});

// ---------------------------------------------------------------------------
// clampTerminalOverlayPaneSize
// ---------------------------------------------------------------------------

describe("clampTerminalOverlayPaneSize", () => {
  it("clamps width and height to the configured minimum when input is below the floor", () => {
    // When
    const result = clampTerminalOverlayPaneSize(50, 20, 800, 600);

    // Then
    expect(result.width).toBe(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX);
    expect(result.height).toBe(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX);
  });

  it("clamps width and height to the viewport maximum when input exceeds the ceiling", () => {
    // When
    const result = clampTerminalOverlayPaneSize(2000, 2000, 400, 300);

    // Then
    expect(result.width).toBe(400);
    expect(result.height).toBe(300);
  });

  it("enforces the minimum when the viewport max is itself below the minimum size", () => {
    // When
    const result = clampTerminalOverlayPaneSize(100, 40, 100, 40);

    // Then
    expect(result.width).toBe(TERMINAL_OVERLAY_PANE_MIN_WIDTH_PX);
    expect(result.height).toBe(TERMINAL_OVERLAY_PANE_MIN_HEIGHT_PX);
  });
});

// ---------------------------------------------------------------------------
// nextPresentationFromAttach — new session
// ---------------------------------------------------------------------------

describe("new-session attach opens overlay without pushing a /terminal route", () => {
  it("opening a new session from hidden state uses the overlay presentation", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "new");

    // Then
    expect(result.presentation).toBe("overlay");
  });

  it("opening a new session does not push to /terminal/:id", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "new");

    // Then
    expect(result.shouldPushTerminalRoute).toBe(false);
  });

  it("opening a new session from any prior presentation still uses overlay", () => {
    // When
    const result = nextPresentationFromAttach("full", "new");

    // Then
    expect(result.presentation).toBe("overlay");
  });
});

// ---------------------------------------------------------------------------
// nextPresentationFromAttach — reconnect
// ---------------------------------------------------------------------------

describe("reconnect attach opens overlay without pushing a /terminal route", () => {
  it("reconnecting from hidden state uses the overlay presentation", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "reconnect");

    // Then
    expect(result.presentation).toBe("overlay");
  });

  it("reconnecting does not push to /terminal/:id", () => {
    // When
    const result = nextPresentationFromAttach("hidden", "reconnect");

    // Then
    expect(result.shouldPushTerminalRoute).toBe(false);
  });

  it("reconnecting while already in overlay presentation stays in overlay", () => {
    // When
    const result = nextPresentationFromAttach("overlay", "reconnect");

    // Then
    expect(result.presentation).toBe("overlay");
  });
});

// ---------------------------------------------------------------------------
// applyOverlayPreviewClickToFull
// ---------------------------------------------------------------------------

describe("expanding overlay preview to full does not trigger a new session connect", () => {
  it("preserves the connect-session call count when expanding to full", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 2, resumeSessionCalls: 1 });

    // When
    const { presentation, counters } = applyOverlayPreviewClickToFull(before);

    // Then
    expect(presentation).toBe("full");
    expect(counters.connectSessionCalls).toBe(before.connectSessionCalls);
  });

  it("preserves the resume-session call count when expanding to full", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 2, resumeSessionCalls: 1 });

    // When
    const { counters } = applyOverlayPreviewClickToFull(before);

    // Then
    expect(counters.resumeSessionCalls).toBe(before.resumeSessionCalls);
  });

  it("preserves the disconnect call count when expanding to full", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 2, disconnectCalls: 0 });

    // When
    const { counters } = applyOverlayPreviewClickToFull(before);

    // Then
    expect(counters.disconnectCalls).toBe(before.disconnectCalls);
  });
});

// ---------------------------------------------------------------------------
// applyDedicatedTerminalBackToMini
// ---------------------------------------------------------------------------

describe("Back from full terminal minimizes to mini without disconnecting", () => {
  it("sets the presentation to mini", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 1 });

    // When
    const { presentation } = applyDedicatedTerminalBackToMini(before);

    // Then
    expect(presentation).toBe("mini");
  });

  it("leaves the disconnect count unchanged", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 1 });

    // When
    const { counters } = applyDedicatedTerminalBackToMini(before);

    // Then
    expect(counters.disconnectCalls).toBe(0);
  });

  it("preserves the connect-session call count", () => {
    // Given
    const before = aConnectCounter({ connectSessionCalls: 1 });

    // When
    const { counters } = applyDedicatedTerminalBackToMini(before);

    // Then
    expect(counters.connectSessionCalls).toBe(before.connectSessionCalls);
  });
});

// ---------------------------------------------------------------------------
// reconcileReconnectOverlayInstances — idempotency
// ---------------------------------------------------------------------------

describe("reconcileReconnectOverlayInstances collapses multiple signals to one overlay", () => {
  it("a single signal yields one overlay instance", () => {
    expect(reconcileReconnectOverlayInstances(1)).toBe(1);
  });

  it("three signals still yield only one overlay instance", () => {
    expect(reconcileReconnectOverlayInstances(3)).toBe(1);
  });

  it("one hundred signals still yield only one overlay instance", () => {
    expect(reconcileReconnectOverlayInstances(100)).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// attachKindForSessionControl
// ---------------------------------------------------------------------------

describe("attachKindForSessionControl maps session control actions to attach kind", () => {
  it("startSession is a new attach", () => {
    expect(attachKindForSessionControl("startSession")).toBe("new");
  });

  it("connectSession is a new attach", () => {
    expect(attachKindForSessionControl("connectSession")).toBe("new");
  });

  it("resumeSession is a reconnect attach", () => {
    expect(attachKindForSessionControl("resumeSession")).toBe("reconnect");
  });
});

// ---------------------------------------------------------------------------
// defaultTerminalMiniOverlayPlacement
// ---------------------------------------------------------------------------

describe("default mini overlay placement", () => {
  it("defaults to floating bottom-right", () => {
    // When
    const placement = defaultTerminalMiniOverlayPlacement();

    // Then
    expect(placement).toBe("bottom-right");
  });
});
