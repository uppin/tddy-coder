import { nodeChildSession } from "./nodeChildSession";
import type { StackChildSession } from "./stackChildSessions";
import type { StackNode } from "./stackPlan";

/**
 * The plan's nodes with the `branch` and `session_id` their own host never got to write filled in
 * from the child session that is live on another one.
 *
 * `link_stack_node_to_spawned_branch` writes the orchestrator's `changeset.yaml` on the **spawning**
 * daemon's sessions tree. Spawn on host B under an orchestrator on host A and there is no such
 * session there, so the write is skipped and the node stays branchless forever — which wedges every
 * descendant, since `Stack::base_ref_for_spawn` gates on a parent owning a branch.
 *
 * Hydrating here, once, is what makes the rest of the screen correct without special cases: base
 * resolution, the spawn gate, the poll set and the row's own branch line all read `node.branch`. And
 * a branch a live child reports **exists** — it is a real ref on a host this daemon cannot see, which
 * is precisely what separates it from a `branch_suggestion`, a planned name that refers to nothing
 * (D1) and is never hydrated here.
 *
 * The plan wins wherever it has an answer: it is the durable record, and a participant republishing a
 * stale block must not be able to move a node onto a branch the plan disagrees about. Only a field
 * the node records as empty is adopted.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37–D39).
 */
export function hydrateStackNodes(
  nodes: StackNode[],
  children: StackChildSession[],
  orchestratorSessionId: string,
): StackNode[] {
  return nodes.map((node) => {
    const child = nodeChildSession(node, children, orchestratorSessionId);
    if (!child) return node;
    const branch = node.branch || child.branch || null;
    const sessionId = node.sessionId || child.sessionId || null;
    if (branch === node.branch && sessionId === node.sessionId) return node;
    return { ...node, branch, sessionId };
  });
}
