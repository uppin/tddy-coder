import { describe, expect, it } from "bun:test";
import { defaultInspectorTab } from "./defaultInspectorTab";

describe("defaultInspectorTab", () => {
  it("opens on PR when the session's branch has a pull request, even when it owns a work queue", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: true, prNumber: 7n })).toBe("pr");
  });

  it("opens on PR for a session without a queue when its branch has a pull request", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: false, prNumber: 7n })).toBe("pr");
  });

  it("opens on Worktree when there is neither a queue nor a pull request", () => {
    expect(defaultInspectorTab({ ownsWorkQueue: false, prNumber: 0n })).toBe("worktree");
  });
});
