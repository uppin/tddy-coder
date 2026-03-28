import { describe, expect, it } from "bun:test";
import {
  formatSessionCreatedAt,
  sessionIdFirstSegment,
  sessionPidDisplay,
} from "./sessionDisplay";

describe("sessionIdFirstSegment", () => {
  it("returns first UUID segment (8 hex chars)", () => {
    expect(sessionIdFirstSegment("550e8400-e29b-41d4-a716-446655440000")).toBe("550e8400");
  });

  it("returns substring before first hyphen for non-UUID ids", () => {
    expect(sessionIdFirstSegment("session-active-1")).toBe("session");
  });

  it("returns whole id when there is no hyphen", () => {
    expect(sessionIdFirstSegment("nohyphen")).toBe("nohyphen");
  });

  it("trims whitespace", () => {
    expect(sessionIdFirstSegment("  abc-def  ")).toBe("abc");
  });

  it("returns em dash for empty after trim", () => {
    expect(sessionIdFirstSegment("   ")).toBe("—");
  });
});

describe("formatSessionCreatedAt", () => {
  it("formats RFC3339 UTC as YYYY-MM-DD HH:MM:SS in UTC", () => {
    expect(formatSessionCreatedAt("2026-03-21T10:00:00Z")).toBe("2026-03-21 10:00:00");
  });

  it("returns original string when parse fails", () => {
    expect(formatSessionCreatedAt("not-a-date")).toBe("not-a-date");
  });
});

describe("sessionPidDisplay", () => {
  it("shows pid when active and pid > 0", () => {
    expect(sessionPidDisplay(true, 12345)).toBe("12345");
  });

  it("returns em dash when inactive even if pid is set", () => {
    expect(sessionPidDisplay(false, 12345)).toBe("—");
  });

  it("returns em dash when active but pid is zero", () => {
    expect(sessionPidDisplay(true, 0)).toBe("—");
  });
});
