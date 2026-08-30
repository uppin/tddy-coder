import type { SessionEntry } from "../../../gen/connection_pb";
import type { SessionMetadata } from "../../../lib/sessionParticipantMetadata";

/**
 * A session the PR-Stack view may join to a planned node: which session it is, which orchestrator
 * spawned it, which planned node it materializes, and which branch it created.
 *
 * A domain type of the view's own rather than a `SessionEntry` (D38). Three of the four facts already
 * ride on the proto row; `stackNodeId` exists only in the participant's `session` metadata block,
 * because it is needed exactly where a participant is live — a same-host child with no participant
 * needs no such join, since the node's own link was written correctly on its own host. Keeping the
 * join off the proto row also stops it being coupled to the `ListSessions` shape.
 */
export interface StackChildSession {
  sessionId: string;
  orchestratorSessionId: string;
  stackNodeId: string;
  branch: string;
  isActive: boolean;
}

/**
 * The stack children among `sessions`, assembled from each row plus the participant metadata the
 * drawer already parses (`sessionMetadataBySessionId`).
 *
 * Each fact is taken from whichever source is authoritative for it:
 *
 * - **`branch` and `orchestratorSessionId`: the session row first.** `ListSessions` reads
 *   `changeset.yaml` on the session's own host, which is where both are written; the metadata block
 *   is a copy that a reconnect can republish stale. The metadata answers only where the row cannot —
 *   a synthesized cross-host row before `mergeActiveAndFetchedSessions` hydrates it, or a row that
 *   never was.
 * - **`stackNodeId`: the metadata alone.** It is not a `SessionEntry` field, deliberately (D38).
 *
 * A session naming **no orchestrator is dropped**: it is not a stack child. A node id on its own
 * would let one stack's row claim another stack's session, because node ids are unique within a
 * single plan and nowhere else.
 *
 * `isActive` is carried alongside rather than filtered on: liveness decides the in-progress badge,
 * never whether the branch a finished child created still exists.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D37, D38).
 */
export function stackChildSessions(
  sessions: SessionEntry[],
  metadataBySessionId: ReadonlyMap<string, SessionMetadata>,
): StackChildSession[] {
  const children: StackChildSession[] = [];
  for (const entry of sessions) {
    const meta = metadataBySessionId.get(entry.sessionId);
    const orchestratorSessionId = entry.orchestratorSessionId || meta?.orchestratorSessionId || "";
    if (!orchestratorSessionId) continue;
    children.push({
      sessionId: entry.sessionId,
      orchestratorSessionId,
      stackNodeId: meta?.stackNodeId ?? "",
      branch: entry.branch || meta?.branch || "",
      isActive: entry.isActive,
    });
  }
  return children;
}
