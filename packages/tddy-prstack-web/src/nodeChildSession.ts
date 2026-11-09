import type { StackChildSession } from "./stackChildSessions";
import type { StackNode } from "./stackPlan";

/** A live child beats a finished one within a leg — the same tie-break `resolveNodeSession` makes. */
function preferActive(candidates: StackChildSession[]): StackChildSession | undefined {
  return candidates.find((child) => child.isActive) ?? candidates[0];
}

/**
 * The children of `orchestratorSessionId`, or none at all when the caller cannot say which stack it
 * is asking about — an unnamed orchestrator would let a node id alone decide, and node ids are
 * unique within one plan and nowhere else.
 */
function childrenOfStack(
  children: StackChildSession[],
  orchestratorSessionId: string,
): StackChildSession[] {
  if (!orchestratorSessionId) return [];
  return children.filter((child) => child.orchestratorSessionId === orchestratorSessionId);
}

/**
 * The child session that **is** `node`'s child, by identity alone — the first two legs of
 * {@link nodeChildSession} without the branch-ownership leg.
 *
 * 1. **The child that names the node.** `stackNodeId` plus `orchestratorSessionId` is an exact
 *    identity: it survives the operator renaming the branch in the create dialog, and it survives the
 *    host boundary — the case the branch join cannot reach at all, since a cross-host node's link was
 *    written on the child's own disk and this plan never learned it.
 * 2. **The child the plan records.** The durable record of the same fact.
 *
 * Both legs answer "this node's child exists", which is the only evidence that may override
 * `QueryBranch`'s orphan verdict (D40): a resolution reporting `session.exists = false` means "not on
 * the daemon I asked", and only a positive claim on *this* node can outrank it.
 *
 * The branch leg deliberately does not appear. "Whoever owns this branch right now" is a different
 * question, and its answer is routinely a session that has nothing to do with the node's record — a
 * fresh session in the same stack picking the branch up after the recorded child was deleted is
 * exactly the D7 orphan the recovery CTA exists for. Letting that suppress the verdict would leave
 * the row claiming to show a child it no longer has, with no way to start a new one.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D39, D40).
 */
export function nodeChildSessionByIdentity(
  node: StackNode,
  children: StackChildSession[],
  orchestratorSessionId: string,
): StackChildSession | undefined {
  const ours = childrenOfStack(children, orchestratorSessionId);

  // An empty half never matches: a node id alone would let one stack's row claim another stack's
  // session, and an unnamed orchestrator would let it claim every stack's.
  const claimsNode = ours.filter(
    (child) => child.stackNodeId !== "" && child.stackNodeId === node.nodeId,
  );
  if (claimsNode.length > 0) return preferActive(claimsNode);

  const recorded = node.sessionId ?? "";
  if (recorded) {
    const byRecord = ours.filter((child) => child.sessionId === recorded);
    if (byRecord.length > 0) return preferActive(byRecord);
  }

  return undefined;
}

/**
 * Which child session is working `node`, joined over the whole fleet rather than one host's
 * `ListSessions`.
 *
 * Three legs, tried in order (D39, extending D23): the two identity legs of
 * {@link nodeChildSessionByIdentity}, then —
 *
 * 3. **Whoever owns the branch right now.** A different question, whose answer changes after a resume
 *    or a hand-off, so it stays last exactly as D23 argued.
 *
 * This is the resolver for everything the row states about the session it is *looking at*: the status
 * chip, the session it opens, and the in-progress badge. All three are about the branch's current
 * worker, which is what leg 3 names correctly.
 *
 * It is **not** the resolver for the orphan verdict — see `isNodeOrphaned`, which takes the identity
 * legs only, because leg 3 proves a session exists without proving it is this node's.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D39).
 */
export function nodeChildSession(
  node: StackNode,
  children: StackChildSession[],
  orchestratorSessionId: string,
): StackChildSession | undefined {
  const byIdentity = nodeChildSessionByIdentity(node, children, orchestratorSessionId);
  if (byIdentity) return byIdentity;

  const branch = node.branch ?? "";
  if (branch) {
    const byBranch = childrenOfStack(children, orchestratorSessionId).filter(
      (child) => child.branch === branch,
    );
    if (byBranch.length > 0) return preferActive(byBranch);
  }

  return undefined;
}
