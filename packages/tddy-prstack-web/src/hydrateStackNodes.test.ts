import { describe, expect, it } from "bun:test";
import { aStackChildSession, aStackNode } from "../../../test-utils";
import type { StackChildSession } from "./stackChildSessions";
import { hydrateStackNodes } from "./hydrateStackNodes";

/**
 * Tests for `hydrateStackNodes` — filling in the `branch` and `session_id` a node's own host never
 * got to write, from the child session that is live on another one.
 *
 * `link_stack_node_to_spawned_branch` writes the orchestrator's `changeset.yaml` on the **spawning**
 * daemon's sessions tree. Spawn on host B under an orchestrator on host A and there is no such
 * session there, so the write is skipped and the node stays branchless forever — which wedges every
 * descendant, since `Stack::base_ref_for_spawn` gates on a parent owning a branch.
 *
 * Hydrating at render is what makes the rest of the screen correct without special cases: base
 * resolution, the spawn gate, the poll set and the row's own branch line all read `node.branch`, and
 * a branch a live child reports **exists** — it is a real ref on a host this daemon cannot see. That
 * is precisely what separates it from a `branch_suggestion`, which names nothing (D1) and is never
 * hydrated here.
 *
 * The plan wins wherever it has an answer: it is the durable record, and a stale participant must
 * not be able to move a node onto a branch the plan disagrees about.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37–D39).
 */

const ORCHESTRATOR = "pr-stack-session-1";
const REMOTE_CHILD = "dddddddd-0000-4000-8000-000000000004";
const REMOTE_BRANCH = "feature/attach-docs/attach-store";

/** The live cross-host child of `n1`: it names the node and owns the branch it created. */
function aChild(overrides: Partial<StackChildSession> = {}) {
  return aStackChildSession({
    sessionId: REMOTE_CHILD,
    orchestratorSessionId: ORCHESTRATOR,
    stackNodeId: "n1",
    branch: REMOTE_BRANCH,
    ...overrides,
  });
}

describe("hydrateStackNodes", () => {
  it("adopts the branch a live child created onto a node that records none", () => {
    // Given — the link was written on the child's host, so this plan never learned the branch
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH })];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.branch).toBe(REMOTE_BRANCH);
  });

  it("adopts the live child's session id onto a node that records none", () => {
    // Given
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH })];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.sessionId).toBe(REMOTE_CHILD);
  });

  it("keeps the session id the plan records when a participant reports a different one", () => {
    // Given — the plan's record is durable; the live child is a restart the plan has not learned of
    const nodes = [
      aStackNode({ nodeId: "n1", branch: REMOTE_BRANCH, sessionId: "child-recorded-by-the-plan" }),
    ];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.sessionId).toBe("child-recorded-by-the-plan");
  });

  it("keeps the branch the plan records when a participant reports a different one", () => {
    // Given — the plan is the durable record; a reconnect can republish a stale block
    const nodes = [aStackNode({ nodeId: "n1", branch: "feature/recorded/by-the-plan" })];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.branch).toBe("feature/recorded/by-the-plan");
  });

  it("unblocks a dependent node once its predecessor's branch is hydrated", () => {
    // Given — n2 waits on n1's branch, which only the live cross-host child knows about
    const nodes = [
      aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH }),
      aStackNode({ nodeId: "n2", parents: ["n1"] }),
    ];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then — the spawn gate reads `branch`, so hydrating it is what makes the chain buildable again
    expect(hydrated.find((n) => n.nodeId === "n1")?.branch).toBe(REMOTE_BRANCH);
  });

  it("never hydrates a branch from a planned name alone", () => {
    // Given — nobody is working the node; its suggestion names no ref (D1)
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH })];

    // When
    const hydrated = hydrateStackNodes(nodes, [], ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.branch).toBeNull();
    expect(hydrated[0]?.sessionId).toBeNull();
  });

  it("leaves a node untouched when the only child claiming its id belongs to another stack", () => {
    // Given
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH })];
    const children = [aChild({ orchestratorSessionId: "pr-stack-session-somewhere-else" })];

    // When
    const hydrated = hydrateStackNodes(nodes, children, ORCHESTRATOR);

    // Then
    expect(hydrated[0]?.branch).toBeNull();
  });

  it("returns the nodes unchanged when no child claims any of them", () => {
    // Given
    const nodes = [
      aStackNode({ nodeId: "n1", branch: "feature/a" }),
      aStackNode({ nodeId: "n2", parents: ["n1"] }),
    ];

    // When
    const hydrated = hydrateStackNodes(nodes, [], ORCHESTRATOR);

    // Then
    expect(hydrated).toEqual(nodes);
  });

  it("hands back the very same node object when it has nothing to adopt", () => {
    // Given — a node whose live child tells it only what it already records
    const nodes = [
      aStackNode({ nodeId: "n1", branch: REMOTE_BRANCH, sessionId: REMOTE_CHILD }),
    ];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild()], ORCHESTRATOR);

    // Then — this runs on every render, and a fresh object per poll tick would invalidate the memos
    // that keep the whole screen (the poll set, the base resolution, every row) off the render path
    expect(hydrated[0]).toBe(nodes[0]);
  });

  it("adopts a branch from a child that has since finished", () => {
    // Given — the session ended, but the branch it created did not
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: REMOTE_BRANCH })];

    // When
    const hydrated = hydrateStackNodes(nodes, [aChild({ isActive: false })], ORCHESTRATOR);

    // Then — liveness decides the "in progress" badge, never whether the branch exists
    expect(hydrated[0]?.branch).toBe(REMOTE_BRANCH);
  });
});
