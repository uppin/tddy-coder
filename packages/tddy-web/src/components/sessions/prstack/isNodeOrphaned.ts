import type { BranchResolution } from "../../../gen/connection_pb";
import type { StackNode } from "./stackPlan";

/**
 * Whether a planned node's recorded child session is gone, making the planned PR workable again.
 *
 * `DeleteSession` removes a session directory without touching the orchestrator's `Changeset.stack`,
 * so a node keeps a dangling `session_id` forever. A row keyed on that field alone therefore shows a
 * status chip for a session that no longer exists, with no control left to start a new one.
 *
 * `QueryBranch` is the authority: it scans sessions by their changeset branch, so a resolution
 * reporting no session for the node's branch means the recorded child is gone.
 *
 * A resolution that has not arrived is *unknown*, never orphaned. `useQueryBranch` swallows failed
 * polls, so reading an absent resolution as an orphan would offer a duplicate spawn for a node whose
 * child is very much alive. A node that owns no branch has no join key at all and never resolves.
 */
export function isNodeOrphaned(
  node: StackNode,
  resolution: BranchResolution | undefined,
): boolean {
  if (!node.sessionId) return false;
  return resolution?.session?.exists === false;
}
