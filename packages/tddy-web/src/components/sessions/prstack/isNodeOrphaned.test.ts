import { describe, expect, it } from "bun:test";
import { aBranchResolution } from "../../../test-utils";
import { isNodeOrphaned } from "./isNodeOrphaned";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `isNodeOrphaned` — whether a planned PR's recorded child session is gone.
 *
 * `DeleteSession` removes a session directory without touching the orchestrator's `Changeset.stack`,
 * so a node keeps a dangling `session_id` forever. The authority on whether that session still
 * exists is `QueryBranch`, which scans sessions by their changeset branch: a resolution reporting no
 * session for the node's branch means the recorded child is gone and the node is workable again.
 *
 * A resolution that has not arrived is "unknown", never "orphaned" — `useQueryBranch` swallows
 * failed polls, so treating an absent resolution as an orphan would offer a duplicate spawn for a
 * node whose child is alive.
 */

const OWNED_BRANCH = "feature/attach-docs/attach-store";

/** A stack node with only the fields these tests care about; everything else gets a valid default. */
function aNode(overrides: Partial<StackNode> & { nodeId: string }): StackNode {
  return {
    nodeId: overrides.nodeId,
    title: overrides.title ?? overrides.nodeId,
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    childRecipe: "tdd",
    internalStatus: null,
    ...overrides,
  };
}

describe("isNodeOrphaned", () => {
  it("reports orphaned when the node records a session the resolution says does not exist", () => {
    // Given — the node was spawned once; nothing owns its branch now
    const node = aNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-since-deleted" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(true);
  });

  it("reports not orphaned when the resolution finds a live session on the branch", () => {
    // Given
    const node = aNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: true, sessionId: "child-n1", isActive: true, status: "active" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned when the resolution finds an idle session on the branch", () => {
    // Given — a session that exists but is not running still owns the node; it can be resumed
    const node = aNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: true, sessionId: "child-n1", isActive: false, status: "idle" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned while the resolution has not arrived", () => {
    // Given — the node records a session but nothing is known about it yet
    const node = aNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: "child-n1" });

    // When
    const orphaned = isNodeOrphaned(node, undefined);

    // Then — an unanswered poll must not be read as a deleted session
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned for a node that was never spawned", () => {
    // Given — no session was ever recorded, so there is nothing to be orphaned from
    const node = aNode({ nodeId: "n1", branchSuggestion: OWNED_BRANCH });
    const resolution = aBranchResolution({
      branch: OWNED_BRANCH,
      session: { exists: false, sessionId: "", isActive: false, status: "" },
    });

    // When
    const orphaned = isNodeOrphaned(node, resolution);

    // Then
    expect(orphaned).toBe(false);
  });

  it("reports not orphaned for a node that records a session but owns no branch", () => {
    // Given — with no branch there is no join key, so `QueryBranch` never resolves this node
    const node = aNode({ nodeId: "n1", sessionId: "child-n1" });

    // When
    const orphaned = isNodeOrphaned(node, undefined);

    // Then
    expect(orphaned).toBe(false);
  });
});
