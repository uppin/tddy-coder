/**
 * Unit tests for the Hosts row's last-seen phrasing.
 *
 * `formatLastSeen` takes `nowUnixMs` as a parameter precisely so a test can pin the instant: every
 * case below is a fixed distance from one frozen reference time, so no phrase depends on when the
 * suite runs.
 */

import { describe, expect, it } from "bun:test";
import { formatLastSeen } from "./hostRowFormat";

/** 2026-09-06T12:00:00Z — arbitrary, but frozen. */
const NOW_MS = 1_788_696_000_000;

const SECOND_MS = 1_000;
const MINUTE_MS = 60 * SECOND_MS;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** A stamp `agoMs` before the frozen reference time. */
function secondsBefore(agoMs: number): bigint {
  return BigInt(NOW_MS - agoMs);
}

describe("formatLastSeen — relative last-seen phrasing", () => {
  it("reads 'just now' under a minute", () => {
    // When
    const phrase = formatLastSeen(secondsBefore(30 * SECOND_MS), NOW_MS);
    // Then
    expect(phrase).toBe("just now");
  });

  it("says '1 minute ago' — singular — at exactly one minute", () => {
    // When
    const phrase = formatLastSeen(secondsBefore(MINUTE_MS), NOW_MS);
    // Then
    expect(phrase).toBe("1 minute ago");
  });

  it("pluralises minutes", () => {
    // When
    const phrase = formatLastSeen(secondsBefore(5 * MINUTE_MS), NOW_MS);
    // Then
    expect(phrase).toBe("5 minutes ago");
  });

  it("rolls up to whole hours, dropping the remainder", () => {
    // When — 90 minutes is one hour and a half, not "90 minutes"
    const phrase = formatLastSeen(secondsBefore(90 * MINUTE_MS), NOW_MS);
    // Then
    expect(phrase).toBe("1 hour ago");
  });

  it("rolls up to whole days", () => {
    // When
    const phrase = formatLastSeen(secondsBefore(3 * DAY_MS), NOW_MS);
    // Then
    expect(phrase).toBe("3 days ago");
  });

  // The daemon's clock and the browser's need not agree to the second, so a stamp can land in the
  // future. "in 4 seconds" would be noise; the elapsed branch must clamp rather than go negative.
  it("reads 'just now' for a stamp slightly in the future", () => {
    // When
    const phrase = formatLastSeen(BigInt(NOW_MS + 4 * SECOND_MS), NOW_MS);
    // Then
    expect(phrase).toBe("just now");
  });

  it("accepts a plain number stamp as well as the wire's bigint", () => {
    // When
    const phrase = formatLastSeen(NOW_MS - 2 * HOUR_MS, NOW_MS);
    // Then
    expect(phrase).toBe("2 hours ago");
  });
});
