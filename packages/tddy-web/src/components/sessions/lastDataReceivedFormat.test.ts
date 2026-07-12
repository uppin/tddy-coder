/**
 * Unit tests for `formatLastDataReceived` — relative-time formatter for the inspector's
 * "last data received: Ns ago" line.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-07-12-fast-session-change.md` (req 5)
 *
 * ⚠️ RED PHASE — fails until `./lastDataReceivedFormat` exists with the API below.
 */

import { describe, it, expect } from "bun:test";
import { formatLastDataReceived } from "./lastDataReceivedFormat";

const NOW = 1_700_000_000_000;

describe("formatLastDataReceived", () => {
  it("returns a 'never' phrase when no data has ever been received", () => {
    expect(formatLastDataReceived(null, NOW)).toBe("never");
  });

  it("formats a sub-minute delta as 'Ns ago'", () => {
    expect(formatLastDataReceived(NOW - 5_000, NOW)).toBe("5s ago");
    expect(formatLastDataReceived(NOW - 1_000, NOW)).toBe("1s ago");
  });

  it("formats a sub-hour delta as 'Nm ago'", () => {
    expect(formatLastDataReceived(NOW - 120_000, NOW)).toBe("2m ago");
    expect(formatLastDataReceived(NOW - 60_000, NOW)).toBe("1m ago");
  });

  it("formats a sub-day delta as 'Nh ago'", () => {
    expect(formatLastDataReceived(NOW - 7_200_000, NOW)).toBe("2h ago");
  });

  it("clamps a future timestamp to 'just now' rather than a negative relative", () => {
    expect(formatLastDataReceived(NOW + 3_000, NOW)).toBe("just now");
  });
});
