/**
 * Unit tests for token-count display formatting.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import { describe, it, expect } from "bun:test";

import { formatTokens } from "./formatTokens";

describe("formatTokens", () => {
  it("renders counts under a thousand without grouping", () => {
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(999)).toBe("999");
  });

  it("groups thousands with commas", () => {
    expect(formatTokens(1000)).toBe("1,000");
    expect(formatTokens(12340)).toBe("12,340");
  });

  it("groups millions with commas", () => {
    expect(formatTokens(1_234_567)).toBe("1,234,567");
  });

  it("formats bigint token counts the same as numbers", () => {
    expect(formatTokens(12340n)).toBe("12,340");
    expect(formatTokens(20_470n)).toBe("20,470");
  });
});
