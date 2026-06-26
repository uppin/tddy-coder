import { describe, it, expect } from "bun:test";
import {
  applyTerminalControlEvent,
  initialTerminalControlState,
  type TerminalControlState,
  type TerminalControlEventData,
} from "./terminalControlState";

// Tests for the pure `applyTerminalControlEvent` reducer.

describe("applyTerminalControlEvent", () => {
  describe("snapshot event — you_are_controller: true", () => {
    it("sets isController=true and holderScreenId to this screen", () => {
      // Given
      const state = initialTerminalControlState;
      const event: TerminalControlEventData = {
        holderScreenId: "my-screen-id",
        youAreController: true,
      };

      // When
      const next = applyTerminalControlEvent(state, event);

      // Then
      expect(next.isController).toBe(true);
      expect(next.holderScreenId).toBe("my-screen-id");
    });
  });

  describe("snapshot event — you_are_controller: false", () => {
    it("sets isController=false and records the holder screen id", () => {
      // Given
      const state = initialTerminalControlState;
      const event: TerminalControlEventData = {
        holderScreenId: "other-screen-xyz",
        youAreController: false,
      };

      // When
      const next = applyTerminalControlEvent(state, event);

      // Then
      expect(next.isController).toBe(false);
      expect(next.holderScreenId).toBe("other-screen-xyz");
    });
  });

  describe("displacement event — another screen stole control", () => {
    it("flips isController to false and updates holder to the new screen", () => {
      // Given — this screen currently holds control
      const state: TerminalControlState = {
        isController: true,
        holderScreenId: "my-screen-id",
      };
      const event: TerminalControlEventData = {
        holderScreenId: "new-controller-screen",
        youAreController: false,
      };

      // When
      const next = applyTerminalControlEvent(state, event);

      // Then
      expect(next.isController).toBe(false);
      expect(next.holderScreenId).toBe("new-controller-screen");
    });
  });

  describe("re-claim event — this screen steals control back", () => {
    it("sets isController=true again after being displaced", () => {
      // Given — another screen currently holds control
      const state: TerminalControlState = {
        isController: false,
        holderScreenId: "other-screen",
      };
      const event: TerminalControlEventData = {
        holderScreenId: "my-screen-id",
        youAreController: true,
      };

      // When
      const next = applyTerminalControlEvent(state, event);

      // Then
      expect(next.isController).toBe(true);
      expect(next.holderScreenId).toBe("my-screen-id");
    });
  });

  describe("immutability", () => {
    it("does not mutate the input state", () => {
      // Given
      const state: TerminalControlState = { isController: true, holderScreenId: "screen-a" };
      const event: TerminalControlEventData = { holderScreenId: "screen-b", youAreController: false };
      const stateBefore = { ...state };

      // When
      applyTerminalControlEvent(state, event);

      // Then
      expect(state).toEqual(stateBefore);
    });
  });
});
