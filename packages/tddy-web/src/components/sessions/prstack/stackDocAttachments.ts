/**
 * The documents the Start-session dialog opens with attached for a planned PR.
 *
 * The web mirror of the daemon's `stack_doc_attachments` (`packages/tddy-daemon/src/`), which feeds
 * the agent's `pr_spawn_child`: a child must not differ by how it was started, so both paths offer
 * the same four documents in the same order — the node's own PRD and changeset, then the stack's
 * shared plan and exploration map.
 *
 * Paths are derived from `node_id` **by convention**; nothing on `StackNode` records them. A document
 * that the orchestrator has not written is skipped rather than fatal — starting a node before the
 * docs pass has run is sometimes correct, and it just carries less context.
 *
 * Every row references the orchestrator's own session (`SESSION_ARTIFACT`), so nothing is uploaded
 * and a stack whose orchestrator lives on another host works unchanged.
 *
 * Feature: docs/ft/coder/pr-stack-docs.md § Auto-attachment in the Start-session dialog
 */

import { HostDocumentScope, type SessionEntry } from "../../../gen/connection_pb";
import { contextDocRelativePath } from "../attachments/contextDocPath";
import type { InitialAttachment } from "../attachments/pendingAttachment";

/** Directory under the orchestrator's `artifacts/` holding one subdirectory per planned node. */
const NODE_DOCS_SUBDIR = "prs";
const NODE_PRD_BASENAME = "PRD.md";
const NODE_CHANGESET_BASENAME = "changeset.md";
const PR_STACK_PLAN_MD_BASENAME = "pr-stack-plan.md";
const EXPLORATION_BASENAME = "exploration.md";

/** One offered document: where it is read from, and the flat name it is stored under. */
interface OfferedDocument {
  relativePath: string;
  basename: string;
}

/** The four documents a node is offered, in listing order. */
function offeredDocuments(nodeId: string): OfferedDocument[] {
  const nodeDir = `${NODE_DOCS_SUBDIR}/${nodeId}`;
  return [
    { relativePath: `${nodeDir}/${NODE_PRD_BASENAME}`, basename: NODE_PRD_BASENAME },
    { relativePath: `${nodeDir}/${NODE_CHANGESET_BASENAME}`, basename: NODE_CHANGESET_BASENAME },
    { relativePath: PR_STACK_PLAN_MD_BASENAME, basename: PR_STACK_PLAN_MD_BASENAME },
    { relativePath: EXPLORATION_BASENAME, basename: EXPLORATION_BASENAME },
  ];
}

/**
 * The attach rows the dialog for `nodeId` opens with — those of the four documents the orchestrator
 * actually holds. An orchestrator whose docs pass has not run yields the shared pair alone, and one
 * that has written nothing yields an empty list.
 */
export function stackDocAttachments(
  orchestrator: SessionEntry,
  nodeId: string,
): InitialAttachment[] {
  const onDisk = new Map(
    orchestrator.contextDocs
      .filter((doc) => doc.exists)
      .map((doc) => [contextDocRelativePath(doc), doc] as const),
  );
  return offeredDocuments(nodeId).flatMap(({ relativePath, basename }) => {
    const doc = onDisk.get(relativePath);
    if (doc === undefined) return [];
    return [
      {
        // The destination is flat: the attachment store is one level deep, so `prs/n1/PRD.md` lands
        // as `PRD.md`.
        basename,
        sizeBytes: Number(doc.sizeBytes),
        source: {
          case: "hostDocument" as const,
          document: {
            // The host owning the orchestrator, which is not necessarily the one running the child.
            daemonInstanceId: orchestrator.daemonInstanceId,
            scope: HostDocumentScope.SESSION_ARTIFACT,
            sessionId: orchestrator.sessionId,
            projectId: "",
            relativePath,
          },
        },
      },
    ];
  });
}
