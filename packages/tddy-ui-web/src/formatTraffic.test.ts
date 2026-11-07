/**
 * Unit tests for traffic-display formatting helpers.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import { describe, it, expect } from "bun:test";

import { formatBytes, formatRate, formatPing } from "./formatTraffic";

// ---------------------------------------------------------------------------
// formatBytes
// ---------------------------------------------------------------------------

describe("formatBytes", () => {
  it("formats 0 as '0 B'", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formats values under 1000 as plain bytes", () => {
    expect(formatBytes(1)).toBe("1 B");
    expect(formatBytes(999)).toBe("999 B");
  });

  it("formats values >= 1000 and < 1_000_000 as kB with one decimal", () => {
    expect(formatBytes(1000)).toBe("1.0 kB");
    expect(formatBytes(1500)).toBe("1.5 kB");
    expect(formatBytes(999_999)).toBe("1000.0 kB");
  });

  it("formats values >= 1_000_000 and < 1_000_000_000 as MB with one decimal", () => {
    expect(formatBytes(1_000_000)).toBe("1.0 MB");
    expect(formatBytes(2_500_000)).toBe("2.5 MB");
  });

  it("formats values >= 1_000_000_000 as GB with one decimal", () => {
    expect(formatBytes(1_000_000_000)).toBe("1.0 GB");
    expect(formatBytes(1_500_000_000)).toBe("1.5 GB");
  });

  it("rounds to one decimal place", () => {
    // 1234 bytes = 1.234 kB → rounds to 1.2 kB
    expect(formatBytes(1234)).toBe("1.2 kB");
  });
});

// ---------------------------------------------------------------------------
// formatRate
// ---------------------------------------------------------------------------

describe("formatRate", () => {
  it("formats 0 as '0 B/s'", () => {
    expect(formatRate(0)).toBe("0 B/s");
  });

  it("appends /s to the base byte representation", () => {
    expect(formatRate(500)).toBe("500 B/s");
    expect(formatRate(1500)).toBe("1.5 kB/s");
    expect(formatRate(2_000_000)).toBe("2.0 MB/s");
  });

  it("uses the same thresholds and rounding as formatBytes", () => {
    // 1234 B/s → 1.2 kB/s
    expect(formatRate(1234)).toBe("1.2 kB/s");
  });
});

// ---------------------------------------------------------------------------
// formatPing
// ---------------------------------------------------------------------------

describe("formatPing", () => {
  it("formats a numeric ms value with a unit suffix", () => {
    expect(formatPing(42)).toBe("42 ms");
    expect(formatPing(0)).toBe("0 ms");
    expect(formatPing(1)).toBe("1 ms");
    expect(formatPing(999)).toBe("999 ms");
  });

  it("formats null as the em-dash placeholder", () => {
    expect(formatPing(null)).toBe("—");
  });

  it("floors fractional ms to integer", () => {
    expect(formatPing(42.7)).toBe("42 ms");
    expect(formatPing(42.9)).toBe("42 ms");
  });
});
