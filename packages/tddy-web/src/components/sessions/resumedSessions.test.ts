import { describe, expect, it } from "bun:test";
import { afterListing, isLive, markResumed, type ResumedSessions } from "./resumedSessions";

const NONE: ResumedSessions = new Set();
const STOPPED = { sessionId: "s1", isActive: false };
const LIVE = { sessionId: "s1", isActive: true };

describe("resumed sessions", () => {
  it("a stopped session is live as soon as it is resumed", () => {
    const resumed = markResumed(NONE, "s1");

    expect(isLive(resumed, STOPPED)).toBe(true);
  });

  it("a resumed session stays live while the listing still reports it stopped", () => {
    const resumed = afterListing(markResumed(NONE, "s1"), [STOPPED]);

    expect(isLive(resumed, STOPPED)).toBe(true);
  });

  it("once the listing reports it live, the listing alone decides again", () => {
    const caughtUp = afterListing(markResumed(NONE, "s1"), [LIVE]);

    expect(isLive(caughtUp, STOPPED)).toBe(false);
  });

  it("a session nobody resumed is as live as the listing says", () => {
    expect(isLive(NONE, STOPPED)).toBe(false);
    expect(isLive(NONE, LIVE)).toBe(true);
  });
});
