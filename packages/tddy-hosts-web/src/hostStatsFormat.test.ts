/**
 * Unit tests for the Host Stats Footer formatting helpers.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import { describe, it, expect } from "bun:test";

import { formatDiskFree, clampCorePercent } from "./hostStatsFormat";

// ---------------------------------------------------------------------------
// formatDiskFree
// ---------------------------------------------------------------------------

describe("formatDiskFree", () => {
  it("formats a GB-scale free byte count with a 'free' suffix", () => {
    expect(formatDiskFree(42_100_000_000)).toBe("42.1 GB free");
  });

  it("accepts a bigint (the proto uint64 available_bytes)", () => {
    expect(formatDiskFree(42_100_000_000n)).toBe("42.1 GB free");
  });

  it("formats zero free space as '0 B free'", () => {
    expect(formatDiskFree(0)).toBe("0 B free");
  });
});

// ---------------------------------------------------------------------------
// clampCorePercent
// ---------------------------------------------------------------------------

describe("clampCorePercent", () => {
  it("passes through values already within range", () => {
    expect(clampCorePercent(0)).toBe(0);
    expect(clampCorePercent(55)).toBe(55);
    expect(clampCorePercent(100)).toBe(100);
  });

  it("clamps a negative value up to 0", () => {
    expect(clampCorePercent(-5)).toBe(0);
  });

  it("clamps a value above 100 down to 100", () => {
    expect(clampCorePercent(140)).toBe(100);
  });
});
