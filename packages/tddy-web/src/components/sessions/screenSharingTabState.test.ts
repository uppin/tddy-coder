/**
 * Unit tests for the screen sharing tab reducer.
 */

import { describe, expect, it } from "bun:test";
import {
  applyScreenSharingTabAction,
  initialScreenSharingTabState,
  type ScreenSharingTarget,
} from "./screenSharingTabState";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

function aVncTarget(overrides: Partial<ScreenSharingTarget> = {}): ScreenSharingTarget {
  return {
    id: "t-vnc-001",
    label: "VNC Dev Box",
    host: "192.168.1.10",
    port: 5900,
    protocol: "vnc",
    ...overrides,
  };
}

function anRdpTarget(overrides: Partial<ScreenSharingTarget> = {}): ScreenSharingTarget {
  return {
    id: "t-rdp-001",
    label: "Windows Server",
    host: "10.0.0.10",
    port: 3389,
    protocol: "rdp",
    ...overrides,
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("applyScreenSharingTabAction", () => {
  it("set_targets replaces the target list with the provided targets", () => {
    // Given
    const vnc = aVncTarget();
    const rdp = anRdpTarget();

    // When
    const state = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "set_targets",
      targets: [vnc, rdp],
    });

    // Then
    expect(state.targets).toHaveLength(2);
    expect(state.targets[0].id).toBe(vnc.id);
    expect(state.targets[0].protocol).toBe("vnc");
    expect(state.targets[1].id).toBe(rdp.id);
    expect(state.targets[1].protocol).toBe("rdp");
  });

  it("add_target appends a VNC target to the list", () => {
    // Given
    const vnc = aVncTarget();

    // When
    const state = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "add_target",
      target: vnc,
    });

    // Then
    expect(state.targets).toHaveLength(1);
    expect(state.targets[0].protocol).toBe("vnc");
    expect(state.targets[0].id).toBe("t-vnc-001");
  });

  it("add_target appends an RDP target to the list", () => {
    // Given
    const rdp = anRdpTarget();

    // When
    const state = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "add_target",
      target: rdp,
    });

    // Then
    expect(state.targets).toHaveLength(1);
    expect(state.targets[0].protocol).toBe("rdp");
    expect(state.targets[0].id).toBe("t-rdp-001");
  });

  it("remove_target removes the target by id and leaves other targets", () => {
    // Given
    const vnc = aVncTarget({ id: "t-vnc-keep" });
    const rdp = anRdpTarget({ id: "t-rdp-remove" });
    const withTwo = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "set_targets",
      targets: [vnc, rdp],
    });

    // When
    const state = applyScreenSharingTabAction(withTwo, {
      type: "remove_target",
      targetId: "t-rdp-remove",
    });

    // Then
    expect(state.targets).toHaveLength(1);
    expect(state.targets[0].id).toBe("t-vnc-keep");
  });

  it("set_stream_status records the streaming status for the given target id", () => {
    // Given
    const vnc = aVncTarget();
    const withTarget = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "add_target",
      target: vnc,
    });

    // When
    const state = applyScreenSharingTabAction(withTarget, {
      type: "set_stream_status",
      targetId: vnc.id,
      status: "streaming",
    });

    // Then
    expect(state.streamStatus[vnc.id]).toBe("streaming");
  });

  it("state is immutable — actions return new objects and do not mutate the input", () => {
    // Given
    const original = initialScreenSharingTabState;

    // When
    const updated = applyScreenSharingTabAction(original, {
      type: "add_target",
      target: aVncTarget(),
    });

    // Then
    expect(updated).not.toBe(original);
    expect(original.targets).toHaveLength(0);
    expect(updated.targets).toHaveLength(1);
  });

  it("open_overlay records the active overlay target id", () => {
    // Given
    const state = initialScreenSharingTabState;

    // When
    const updated = applyScreenSharingTabAction(state, {
      type: "open_overlay",
      targetId: "t-vnc-001",
    });

    // Then
    expect(updated.activeOverlayTargetId).toBe("t-vnc-001");
  });

  it("close_overlay clears the active overlay target id", () => {
    // Given
    const withOverlay = applyScreenSharingTabAction(initialScreenSharingTabState, {
      type: "open_overlay",
      targetId: "t-vnc-001",
    });
    expect(withOverlay.activeOverlayTargetId).toBe("t-vnc-001");

    // When
    const state = applyScreenSharingTabAction(withOverlay, { type: "close_overlay" });

    // Then
    expect(state.activeOverlayTargetId).toBeNull();
  });
});
