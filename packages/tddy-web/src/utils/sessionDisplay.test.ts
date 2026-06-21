import { describe, expect, it } from "bun:test";
import {
  formatSessionCreatedAt,
  sessionIdFirstSegment,
  sessionPidDisplay,
} from "./sessionDisplay";

describe("sessionIdFirstSegment", () => {
  it("returns the first two UUID fields (8 hex + hyphen + 4 hex) in lowercase", () => {
    // When
    const result = sessionIdFirstSegment("550e8400-e29b-41d4-a716-446655440000");
    // Then
    expect(result).toBe("550e8400-e29b");
  });

  it("returns the timestamp prefix for UUID v7 ids", () => {
    // When
    const result = sessionIdFirstSegment("019d390c-ac3e-74b1-97fb-86ea64b8ca8d");
    // Then
    expect(result).toBe("019d390c-ac3e");
  });

  it("returns the substring before the first hyphen for non-UUID ids", () => {
    // When
    const result = sessionIdFirstSegment("session-active-1");
    // Then
    expect(result).toBe("session");
  });

  it("returns the whole id when there is no hyphen", () => {
    // When
    const result = sessionIdFirstSegment("nohyphen");
    // Then
    expect(result).toBe("nohyphen");
  });

  it("trims surrounding whitespace before extracting the first segment", () => {
    // When
    const result = sessionIdFirstSegment("  abc-def  ");
    // Then
    expect(result).toBe("abc");
  });

  it("returns an em dash when the id is blank after trimming", () => {
    // When
    const result = sessionIdFirstSegment("   ");
    // Then
    expect(result).toBe("—");
  });
});

describe("formatSessionCreatedAt", () => {
  it("formats an RFC 3339 UTC timestamp as YYYY-MM-DD HH:MM:SS in UTC", () => {
    // When
    const result = formatSessionCreatedAt("2026-03-21T10:00:00Z");
    // Then
    expect(result).toBe("2026-03-21 10:00:00");
  });

  it("returns the original string when the input cannot be parsed as a date", () => {
    // When
    const result = formatSessionCreatedAt("not-a-date");
    // Then
    expect(result).toBe("not-a-date");
  });
});

describe("sessionPidDisplay", () => {
  it("shows the pid number when the session is active and the pid is positive", () => {
    // When
    const result = sessionPidDisplay(true, 12345);
    // Then
    expect(result).toBe("12345");
  });

  it("shows an em dash when the session is inactive, even if a pid value is set", () => {
    // When
    const result = sessionPidDisplay(false, 12345);
    // Then
    expect(result).toBe("—");
  });

  it("shows an em dash when the session is active but the pid is zero", () => {
    // When
    const result = sessionPidDisplay(true, 0);
    // Then
    expect(result).toBe("—");
  });
});
