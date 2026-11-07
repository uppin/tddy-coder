import { describe, expect, it } from "bun:test";
import { aStackChildSession, aStackNode } from "../../../test-utils";
import { nodeChildSession, nodeChildSessionByIdentity } from "./nodeChildSession";

/**
 * Tests for `nodeChildSession` — which child session a planned node is being worked by, joined over
 * the whole fleet rather than one host's `ListSessions`.
 *
 * Three legs, in this order (D39, extending D23):
 *
 * 1. **The child that names the node.** `stack_node_id` plus `orchestrator_session_id` is an exact
 *    identity: it survives the operator renaming the branch in the create dialog, and it survives the
 *    host boundary, which is the case the branch join cannot reach.
 * 2. **The child the plan records.** The durable record, and what D23 already put first.
 * 3. **Whoever owns the branch right now.** A different question, whose answer changes after a
 *    resume or a hand-off — so it stays last, exactly as D23 argued.
 *
 * Both halves of leg 1 must match. Node ids are unique within one plan and nowhere else, so matching
 * on the node id alone would let one stack's row claim another stack's session.
 *
 * `nodeChildSessionByIdentity` is the first two legs alone — the evidence the orphan verdict is
 * entitled to (D40). Its own scenarios are at the bottom of this file.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D39).
 */

const ORCHESTRATOR = "pr-stack-session-1";
const OWNED_BRANCH = "feature/attach-docs/attach-store";

const CLAIMS_THE_NODE = "child-that-names-the-node";
const RECORDED = "child-session-recorded-by-the-plan";
const BRANCH_OWNER = "child-session-that-owns-the-branch-now";

describe("nodeChildSession", () => {
  it("resolves the child that names the node", () => {
    // Given — the node records nothing: its link was written on the child's own host
    const node = aStackNode({ nodeId: "n1" });
    const children = [aStackChildSession({ sessionId: CLAIMS_THE_NODE, stackNodeId: "n1" })];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(CLAIMS_THE_NODE);
  });

  it("prefers the child that names the node over the one the plan records", () => {
    // Given — a restart on another host left the plan's record pointing at the previous session
    const node = aStackNode({ nodeId: "n1", sessionId: RECORDED, branch: OWNED_BRANCH });
    const children = [
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "", branch: OWNED_BRANCH }),
      aStackChildSession({ sessionId: CLAIMS_THE_NODE, stackNodeId: "n1" }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then — an exact identity beats a record that a restart can outdate
    expect(child?.sessionId).toBe(CLAIMS_THE_NODE);
  });

  it("resolves the child the plan records when nobody names the node", () => {
    // Given — a child spawned before the association existed
    const node = aStackNode({ nodeId: "n1", sessionId: RECORDED, branch: OWNED_BRANCH });
    const children = [
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "", branch: OWNED_BRANCH }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(RECORDED);
  });

  it("falls back to whoever owns the branch when the plan records no session", () => {
    // Given — the node was linked by branch alone
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH });
    const children = [
      aStackChildSession({ sessionId: BRANCH_OWNER, stackNodeId: "", branch: OWNED_BRANCH }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(BRANCH_OWNER);
  });

  it("ignores a child naming the same node under a different orchestrator", () => {
    // Given — node ids are unique within a plan and nowhere else
    const node = aStackNode({ nodeId: "n1" });
    const children = [
      aStackChildSession({
        sessionId: CLAIMS_THE_NODE,
        stackNodeId: "n1",
        orchestratorSessionId: "pr-stack-session-somewhere-else",
      }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child).toBeUndefined();
  });

  it("resolves nothing from a child that claims no node and owns an unrelated branch", () => {
    // Given — a stack child of this orchestrator that materializes some other node
    const node = aStackNode({ nodeId: "n1" });
    const children = [aStackChildSession({ stackNodeId: "", branch: "feature/unrelated" })];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then — nothing this node records or owns matches it
    expect(child).toBeUndefined();
  });

  it("ignores a child whose node id is empty even when the node itself is unnamed", () => {
    // Given — a plan node with no id, and a child that publishes none: comparing the two as values
    // alone would make them equal, which is why the non-empty check exists
    const node = aStackNode({ nodeId: "" });
    const children = [aStackChildSession({ stackNodeId: "", branch: "feature/unrelated" })];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then — empty is "no association", never a wildcard
    expect(child).toBeUndefined();
  });

  it("resolves nothing when the orchestrator itself is unnamed", () => {
    // Given — a caller that cannot say which stack it is asking about
    const node = aStackNode({ nodeId: "n1" });
    const children = [aStackChildSession({ sessionId: CLAIMS_THE_NODE, stackNodeId: "n1" })];

    // When
    const child = nodeChildSession(node, children, "");

    // Then
    expect(child).toBeUndefined();
  });

  it("prefers a live child over a finished one that names the same node", () => {
    // Given — a node restarted on another host, with the previous child's participant still listed
    const node = aStackNode({ nodeId: "n1" });
    const children = [
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "n1", isActive: false }),
      aStackChildSession({ sessionId: CLAIMS_THE_NODE, stackNodeId: "n1", isActive: true }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(CLAIMS_THE_NODE);
  });

  it("prefers the live copy of the child the plan records", () => {
    // Given — the recorded child listed twice: the fetched row this host holds, and the live one
    // presence synthesized for it
    const node = aStackNode({ nodeId: "n1", sessionId: RECORDED });
    const children = [
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "", isActive: false }),
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "", isActive: true }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then — the in-progress badge reads this child's liveness, so a stale copy must not win
    expect(child?.isActive).toBe(true);
  });

  it("prefers a live child over a finished one that owns the same branch", () => {
    // Given — a branch worked by a finished session and picked up again by a live one
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH });
    const children = [
      aStackChildSession({
        sessionId: RECORDED,
        stackNodeId: "",
        branch: OWNED_BRANCH,
        isActive: false,
      }),
      aStackChildSession({
        sessionId: BRANCH_OWNER,
        stackNodeId: "",
        branch: OWNED_BRANCH,
        isActive: true,
      }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(BRANCH_OWNER);
  });

  it("resolves nothing for a node with no branch, no record and nobody claiming it", () => {
    // Given
    const node = aStackNode({ nodeId: "n1" });

    // When
    const child = nodeChildSession(node, [], ORCHESTRATOR);

    // Then
    expect(child).toBeUndefined();
  });
});

/**
 * `nodeChildSessionByIdentity` — the two legs that identify *this node's* child, without the branch
 * leg.
 *
 * The branch leg answers a different question: "who owns this branch right now". Its answer is a
 * session that may have nothing to do with the node's recorded child, so it is not evidence that the
 * recorded child still exists — and treating it as such re-opens the dead end D7 exists to remove.
 */
describe("nodeChildSessionByIdentity", () => {
  it("resolves the child that names the node", () => {
    // Given
    const node = aStackNode({ nodeId: "n1" });
    const children = [aStackChildSession({ sessionId: CLAIMS_THE_NODE, stackNodeId: "n1" })];

    // When
    const child = nodeChildSessionByIdentity(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(CLAIMS_THE_NODE);
  });

  it("resolves the child the plan records", () => {
    // Given
    const node = aStackNode({ nodeId: "n1", sessionId: RECORDED, branch: OWNED_BRANCH });
    const children = [
      aStackChildSession({ sessionId: RECORDED, stackNodeId: "", branch: OWNED_BRANCH }),
    ];

    // When
    const child = nodeChildSessionByIdentity(node, children, ORCHESTRATOR);

    // Then
    expect(child?.sessionId).toBe(RECORDED);
  });

  it("resolves nothing when a fresh session has merely picked up the node's branch", () => {
    // Given — the recorded child was deleted and another session in the same stack took its branch
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: RECORDED });
    const children = [
      aStackChildSession({ sessionId: BRANCH_OWNER, stackNodeId: "", branch: OWNED_BRANCH }),
    ];

    // When
    const child = nodeChildSessionByIdentity(node, children, ORCHESTRATOR);

    // Then — owning the branch proves a *different* session exists, never that this one does
    expect(child).toBeUndefined();
  });

  it("still resolves the branch owner through the full three-leg join", () => {
    // Given — the same fleet, asked the question the status chip asks
    const node = aStackNode({ nodeId: "n1", branch: OWNED_BRANCH, sessionId: RECORDED });
    const children = [
      aStackChildSession({ sessionId: BRANCH_OWNER, stackNodeId: "", branch: OWNED_BRANCH }),
    ];

    // When
    const child = nodeChildSession(node, children, ORCHESTRATOR);

    // Then — the two functions differ in exactly this leg, and nothing else
    expect(child?.sessionId).toBe(BRANCH_OWNER);
  });
});
