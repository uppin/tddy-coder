import { describe, expect, it } from "bun:test";
import { POLL_INTERVAL_MS, nextQueryBranchDelayMs } from "./queryBranchPacing";

/**
 * How long the PR-Stack view waits before asking `QueryBranch` again, given how many polls in a row
 * answered exactly what the view already showed. A stack nobody is changing must not cost the daemon
 * a lookup per branch every five seconds for hours.
 */
describe("nextQueryBranchDelayMs", () => {
  it("polls at the base interval while answers keep changing", () => {
    expect(nextQueryBranchDelayMs(0)).toBe(POLL_INTERVAL_MS);
  });

  it("stays at the base interval for the first few unchanged answers", () => {
    expect(nextQueryBranchDelayMs(2)).toBe(POLL_INTERVAL_MS);
  });

  it("backs off once the stack has been quiet for a while", () => {
    expect(nextQueryBranchDelayMs(3)).toBe(15_000);
    expect(nextQueryBranchDelayMs(6)).toBe(30_000);
  });

  it("never waits longer than a minute", () => {
    expect(nextQueryBranchDelayMs(12)).toBe(60_000);
    expect(nextQueryBranchDelayMs(10_000)).toBe(60_000);
  });
});
