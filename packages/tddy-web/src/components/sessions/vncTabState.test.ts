import { describe, it, expect } from "bun:test";
import {
  applyVncTabAction,
  initialVncTabState,
  type VncTabState,
  type VncTarget,
} from "./vncTabState";

// Tests for the pure `applyVncTabAction` reducer.

const A_TARGET: VncTarget = {
  id: "t1",
  label: "Dev Box",
  host: "192.168.1.100",
  port: 5900,
};

const ANOTHER_TARGET: VncTarget = {
  id: "t2",
  label: "CI Runner",
  host: "10.0.0.5",
  port: 5901,
};

describe("applyVncTabAction — set_targets", () => {
  it("replaces the target list with the provided targets", () => {
    // Given
    const state = initialVncTabState;

    // When
    const next = applyVncTabAction(state, { type: "set_targets", targets: [A_TARGET] });

    // Then
    expect(next.targets).toHaveLength(1);
    expect(next.targets[0].id).toBe("t1");
    expect(next.targets[0].label).toBe("Dev Box");
  });
});

describe("applyVncTabAction — add_target", () => {
  it("appends the new target to the existing list", () => {
    // Given — state with one target
    const state: VncTabState = {
      ...initialVncTabState,
      targets: [A_TARGET],
    };

    // When
    const next = applyVncTabAction(state, { type: "add_target", target: ANOTHER_TARGET });

    // Then
    expect(next.targets).toHaveLength(2);
    expect(next.targets[1].id).toBe("t2");
  });
});

describe("applyVncTabAction — remove_target", () => {
  it("removes the target with the given id from the list", () => {
    // Given — state with two targets
    const state: VncTabState = {
      ...initialVncTabState,
      targets: [A_TARGET, ANOTHER_TARGET],
    };

    // When
    const next = applyVncTabAction(state, { type: "remove_target", targetId: "t1" });

    // Then
    expect(next.targets).toHaveLength(1);
    expect(next.targets[0].id).toBe("t2");
  });
});

describe("applyVncTabAction — set_stream_status", () => {
  it("sets the streaming status for the given target", () => {
    // Given
    const state: VncTabState = {
      ...initialVncTabState,
      targets: [A_TARGET],
    };

    // When
    const next = applyVncTabAction(state, {
      type: "set_stream_status",
      targetId: "t1",
      status: "streaming",
    });

    // Then
    expect(next.streamStatus["t1"]).toBe("streaming");
  });
});

describe("applyVncTabAction — immutability", () => {
  it("does not mutate the input state", () => {
    // Given
    const state: VncTabState = { ...initialVncTabState, targets: [A_TARGET] };
    const snapshot = { ...state, targets: [...state.targets] };

    // When
    applyVncTabAction(state, { type: "set_vault_locked", locked: false });

    // Then — original state unchanged
    expect(state.targets).toEqual(snapshot.targets);
    expect(state.isVaultLocked).toBe(snapshot.isVaultLocked);
  });
});
