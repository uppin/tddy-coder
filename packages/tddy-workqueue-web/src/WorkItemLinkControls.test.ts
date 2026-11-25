import { describe, expect, it } from "bun:test";
import { linkedSessionLabel } from "./WorkItemLinkControls";

/**
 * Tests for `linkedSessionLabel` — what a row reads for one linked session.
 * The full row rendering is pinned by `WorkItemLinksAcceptance.cy.tsx`, which runs on CI.
 */
describe("linkedSessionLabel", () => {
  it("reads a session of this host as its id alone", () => {
    expect(linkedSessionLabel({ sessionId: "sess-a", daemonInstanceId: "" })).toBe("linked: sess-a");
  });

  it("reads a session of another host with its host", () => {
    expect(linkedSessionLabel({ sessionId: "sess-b", daemonInstanceId: "the-other-host" })).toBe(
      "linked: sess-b on the-other-host",
    );
  });
});
