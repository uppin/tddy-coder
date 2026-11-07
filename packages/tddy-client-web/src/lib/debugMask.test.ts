import { describe, it, expect } from "bun:test";
import { resolveDebugMask } from "./debugMask";

describe("resolveDebugMask", () => {
  it("adopts the config mask on first load (no recorded base)", () => {
    // Given a config mask and nothing persisted yet
    const res = resolveDebugMask({
      configMask: "tddy:term:*",
      storedBase: null,
      storedActive: null,
    });
    // Then the config mask is adopted and recorded as the new base
    expect(res.changed).toBe(true);
    expect(res.activeMask).toBe("tddy:term:*");
    expect(res.base).toBe("tddy:term:*");
  });

  it("keeps the persisted active mask when the config mask is unchanged", () => {
    // Given the config matches the recorded base, but the dev tweaked the active mask live
    const res = resolveDebugMask({
      configMask: "tddy:term:*",
      storedBase: "tddy:term:*",
      storedActive: "tddy:term:write",
    });
    // Then the dev's override survives (no invalidation)
    expect(res.changed).toBe(false);
    expect(res.activeMask).toBe("tddy:term:write");
    expect(res.base).toBe("tddy:term:*");
  });

  it("invalidates and re-adopts when the config mask changes", () => {
    // Given a new config mask different from the recorded base
    const res = resolveDebugMask({
      configMask: "tddy:term:grpc",
      storedBase: "tddy:term:*",
      storedActive: "tddy:term:write",
    });
    // Then the prior override is discarded in favour of the new config mask
    expect(res.changed).toBe(true);
    expect(res.activeMask).toBe("tddy:term:grpc");
    expect(res.base).toBe("tddy:term:grpc");
  });

  it("clears the active mask when config goes from set to unset", () => {
    // Given config debug was removed (now empty) but a base/active are still recorded
    const res = resolveDebugMask({
      configMask: "",
      storedBase: "tddy:term:*",
      storedActive: "tddy:term:*",
    });
    // Then the mask is cleared (null = remove localStorage.debug) and base reset to empty
    expect(res.changed).toBe(true);
    expect(res.activeMask).toBeNull();
    expect(res.base).toBe("");
  });

  it("is a no-op when config is unset and stays unset", () => {
    // Given no config mask and no recorded base
    const res = resolveDebugMask({
      configMask: null,
      storedBase: null,
      storedActive: null,
    });
    // Then nothing changes and there is no active mask
    expect(res.changed).toBe(false);
    expect(res.activeMask).toBeNull();
    expect(res.base).toBe("");
  });

  it("treats whitespace-only masks as unset and trims values", () => {
    // Given padded config + base that are equivalent after trimming
    const res = resolveDebugMask({
      configMask: "  tddy:term:*  ",
      storedBase: "tddy:term:*",
      storedActive: "   ",
    });
    // Then the config is unchanged (trim-equal) and the blank active mask becomes null
    expect(res.changed).toBe(false);
    expect(res.activeMask).toBeNull();
    expect(res.base).toBe("tddy:term:*");
  });
});
