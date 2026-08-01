import type { BranchResolution, SessionEntry } from "../../../gen/connection_pb";
import type { StackNode } from "./stackPlan";

/**
 * The child session a spawned planned PR's indicator opens, or `""` when none can be resolved.
 *
 * The plan's own `session_id` wins (D23): the indicator is the *node's* recorded binding and the plan
 * is the durable record. `QueryBranch`'s resolved session answers a different question — "who owns
 * this branch right now" — whose answer changes after a resume or a hand-off, so it is the fallback
 * rather than the primary.
 *
 * Both legs are guarded on the id naming a session the caller actually knows. A recorded id no host
 * reports (deleted elsewhere, or a branch picked up by a fresh session) would otherwise produce a
 * control that selects nothing, which is worse than no control at all.
 */
export function boundChildSession(
  node: StackNode,
  resolution: BranchResolution | undefined,
  sessions: SessionEntry[],
): string {
  const knownSessionIds = new Set(sessions.map((s) => s.sessionId));
  const recorded = node.sessionId ?? "";
  if (recorded && knownSessionIds.has(recorded)) return recorded;
  const branchOwner = resolution?.session?.exists ? resolution.session.sessionId : "";
  if (branchOwner && knownSessionIds.has(branchOwner)) return branchOwner;
  return "";
}
