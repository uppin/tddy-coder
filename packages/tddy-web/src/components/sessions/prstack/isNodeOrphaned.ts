import type { BranchResolution } from "../../../gen/connection_pb";
import type { StackChildSession } from "./stackChildSessions";
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
 *
 * That authority is **one host's**. The session leg is a `read_dir` over the queried daemon's own
 * sessions directory and carries no `unavailable` discriminator, so `exists = false` means "not on
 * the daemon I asked" — the normal state for a child running one host over. A `childSession` resolved
 * for the node therefore overrides the verdict: presence positively proves the session exists, where
 * the resolution can only report not having found it (D40, amending D7). Its liveness is beside the
 * point — an idle child still exists, and only its own host can say it is gone. Absence of a child
 * falls through to D7 unchanged: presence narrows the rule, it does not remove it.
 *
 * `childSession` must be resolved **by identity** — `nodeChildSessionByIdentity`, which claims the
 * node by node id or is the child the plan records. The full `nodeChildSession` join also answers
 * "whoever owns this branch right now", and that session proves nothing about the node's own child:
 * a recorded child that was deleted while a fresh session in the same stack took over its branch is
 * exactly the D7 orphan this predicate exists to report, and the branch leg would silently answer it
 * "not orphaned".
 */
export function isNodeOrphaned(
  node: StackNode,
  resolution: BranchResolution | undefined,
  childSession?: StackChildSession,
): boolean {
  if (!node.sessionId) return false;
  if (childSession) return false;
  return resolution?.session?.exists === false;
}
