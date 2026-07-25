import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../test-utils";
import { resolveNodeSession } from "./resolveNodeSession";

/**
 * Tests for `resolveNodeSession` — the PR-Stack view's branch→session resolver. A planned node is
 * "in progress" when a live session owns its branch; the branch is the join key (a node's
 * `SessionEntry.branch` is matched against each session's `branch`).
 */
describe("resolveNodeSession", () => {
  it("resolves the live session whose branch matches the node branch", () => {
    // Given — one session working the node's branch, another on a different branch
    const owner = aSessionEntry({ sessionId: "child-n1", branch: "feature/x/n1", isActive: true });
    const other = aSessionEntry({ sessionId: "child-x", branch: "feature/x/other", isActive: true });

    // When
    const resolved = resolveNodeSession({ branch: "feature/x/n1" }, [owner, other]);

    // Then
    expect(resolved?.sessionId).toBe("child-n1");
  });

  it("returns undefined when no session branch matches the node branch", () => {
    // Given — no session works the node's branch
    const sessions = [
      aSessionEntry({ sessionId: "child-a", branch: "feature/x/a", isActive: true }),
      aSessionEntry({ sessionId: "child-b", branch: "feature/x/b", isActive: true }),
    ];

    // When
    const resolved = resolveNodeSession({ branch: "feature/x/n1" }, sessions);

    // Then
    expect(resolved).toBeUndefined();
  });

  it("prefers the active session when multiple sessions share the branch", () => {
    // Given — two sessions on the same branch; only one is live
    const stale = aSessionEntry({ sessionId: "child-stale", branch: "feature/x/n1", isActive: false });
    const live = aSessionEntry({ sessionId: "child-live", branch: "feature/x/n1", isActive: true });

    // When
    const resolved = resolveNodeSession({ branch: "feature/x/n1" }, [stale, live]);

    // Then
    expect(resolved?.sessionId).toBe("child-live");
  });
});
