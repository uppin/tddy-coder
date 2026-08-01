import { describe, expect, it } from "bun:test";
import { aBranchResolution, aSessionEntry, aStackNode } from "../../../test-utils";
import { boundChildSession } from "./boundChildSession";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `boundChildSession` — which session a spawned planned PR's indicator opens.
 *
 * The plan's own `session_id` wins: the indicator is the *node's* recorded binding, and the plan is
 * the durable record. `QueryBranch`'s resolved session answers a different question — "who owns this
 * branch right now" — whose answer changes after a resume or a hand-off.
 *
 * Both legs are guarded on the session actually being one the drawer knows. A recorded id that no
 * host reports (deleted elsewhere, or a branch picked up by a fresh session) would otherwise produce
 * a control that selects nothing, which is worse than no control at all.
 */

const OWNED_BRANCH = "feature/auth/middleware";
const RECORDED = "child-session-recorded-by-the-plan";
const BRANCH_OWNER = "child-session-that-owns-the-branch-now";

/** The one node every scenario here is about: a planned PR that owns {@link OWNED_BRANCH}. */
function aNode(overrides: Partial<StackNode> = {}): StackNode {
  return aStackNode({ title: "Add auth middleware", branch: OWNED_BRANCH, ...overrides });
}

const resolvedTo = (sessionId: string) =>
  aBranchResolution({
    branch: OWNED_BRANCH,
    session: { exists: true, sessionId, isActive: true, status: "active" },
  });

describe("boundChildSession", () => {
  it("binds to the child session the plan records", () => {
    // Given
    const node = aNode({ sessionId: RECORDED });
    const sessions = [aSessionEntry({ sessionId: RECORDED })];

    // When
    const bound = boundChildSession(node, resolvedTo(RECORDED), sessions);

    // Then
    expect(bound).toBe(RECORDED);
  });

  it("binds to the session the plan records even when another session owns the branch", () => {
    // Given — both resolve to known sessions, and they differ
    const node = aNode({ sessionId: RECORDED });
    const sessions = [aSessionEntry({ sessionId: RECORDED }), aSessionEntry({ sessionId: BRANCH_OWNER })];

    // When
    const bound = boundChildSession(node, resolvedTo(BRANCH_OWNER), sessions);

    // Then — the node's own binding is the durable record
    expect(bound).toBe(RECORDED);
  });

  it("binds to the session that owns the branch when the recorded child is not a known session", () => {
    // Given — the plan names a session no host reports
    const node = aNode({ sessionId: "child-session-no-host-reports" });
    const sessions = [aSessionEntry({ sessionId: BRANCH_OWNER })];

    // When
    const bound = boundChildSession(node, resolvedTo(BRANCH_OWNER), sessions);

    // Then
    expect(bound).toBe(BRANCH_OWNER);
  });

  it("binds to nothing when neither the recorded child nor the branch owner is known", () => {
    // Given
    const node = aNode({ sessionId: "child-session-no-host-reports" });

    // When
    const bound = boundChildSession(node, resolvedTo("also-unknown"), []);

    // Then — a control that would select nothing is worse than no control
    expect(bound).toBe("");
  });

  it("binds to nothing while the branch resolution has not arrived and the recorded child is unknown", () => {
    // Given
    const node = aNode({ sessionId: "child-session-no-host-reports" });

    // When
    const bound = boundChildSession(node, undefined, []);

    // Then
    expect(bound).toBe("");
  });

  it("binds to nothing for a node that was never spawned", () => {
    // Given
    const node = aNode({ sessionId: null, branch: null });

    // When
    const bound = boundChildSession(node, undefined, []);

    // Then
    expect(bound).toBe("");
  });

  it("binds to the branch owner for a node that records no session of its own", () => {
    // Given — the node was linked by branch rather than by session id
    const node = aNode({ sessionId: null });
    const sessions = [aSessionEntry({ sessionId: BRANCH_OWNER })];

    // When
    const bound = boundChildSession(node, resolvedTo(BRANCH_OWNER), sessions);

    // Then
    expect(bound).toBe(BRANCH_OWNER);
  });
});
