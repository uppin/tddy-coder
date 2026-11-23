import { describe, expect, it } from "bun:test";
import { defaultInspectorTab } from "./defaultInspectorTab";

describe("defaultInspectorTab (#workqueue 5/14)", () => {
  it("opens a session that owns a work queue on Work queue, even with a PR", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: true, prNumber: 7n })).toBe("work-queue");
  });

  it("opens a session without a queue on PR when it has one", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: false, prNumber: 7n })).toBe("pr");
  });

  it("opens a session with neither on Worktree", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: false, prNumber: 0n })).toBe("worktree");
  });
});
