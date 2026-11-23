import React, { useState } from "react";
import { CONNECTOR_GUTTER, WorkItemConnectors, type ConnectorEdge } from "./WorkItemConnectors";
import { WorkItemDragHandle } from "./WorkItemDragHandle";
import { useItemDrag } from "./useItemDrag";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { BranchResolution } from "tddy-client-web/gen/work_queue_pb";
import { resolveNodeSession } from "./resolveNodeSession";
import { WorkItemRow } from "./WorkItemRow";
import { boundChildSession } from "./boundChildSession";
import { plannedNameBranches, type BranchQuery } from "./branchQueries";
import { resolveWorkItemBase } from "./deriveStackBaseBranch";
import { orderWorkItems } from "./orderWorkItems";
import { parentTitles } from "./parentTitles";
import { nodeChildSession, nodeChildSessionByIdentity } from "./nodeChildSession";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import type { StackChildSession } from "./workItemSessions";
import type { WorkItem } from "./workQueue";

export interface WorkItemListProps {
  nodes: WorkItem[];
  onStartSession: (node: WorkItem) => void;
  startingNodeId: string | null;
  /** All sessions — used to resolve each node's in-progress child session by branch. */
  sessions?: SessionEntry[];
  /**
   * One-call branch resolution (worktree + session + remote + PR) keyed by branch, from
   * `useQueryBranch` — the screen's only source of live branch and PR state. Covers each node's own
   * branch *and* each node's base branch, whose `remote` leg decides whether the node can be started
   * at all.
   */
  branchResolutionByBranch?: Record<string, BranchResolution>;
  /** Repoint a node onto the target its control names (drops the parents that do not own it). */
  onRepoint?: (itemId: string) => void;
  /**
   * The project's default branch (`ProjectEntry.main_branch_ref`) — the base of a root node and the
   * repoint target when no parent can serve as a base. Empty for a legacy project that stores none,
   * which degrades the label only: the daemon resolves the real ref at click time (D20).
   */
  defaultBranch?: string;
  /** The daemon's reason per node whose last repoint was refused or failed, keyed by node id. */
  repointErrorByNodeId?: Record<string, string>;
  /**
   * Nodes with a mutation of their branch in flight — a repoint or a pull. Disables that row's
   * repoint control *and* both of its pull controls, since all three rewrite the same branch's refs.
   */
  branchMutatingNodeIds?: ReadonlySet<string>;
  /** Move a node one position in the persisted reading order. */
  onReorder?: (itemId: string, direction: "up" | "down") => void;
  /** The daemon's reason per node whose last reorder was refused or failed, keyed by node id. */
  reorderErrorByNodeId?: Record<string, string>;
  /** Nodes whose reorder is in flight — both of their controls are disabled until it settles. */
  reorderingNodeIds?: ReadonlySet<string>;
  /** Pull a node's base into its branch, by the strategy the clicked control named. */
  onSyncFromBase?: (itemId: string, strategy: "merge" | "rebase") => void;
  /** The daemon's reason per node whose last pull was refused or failed, keyed by node id. */
  syncErrorByNodeId?: Record<string, string>;
  /**
   * Select and attach a node's bound child session — the same act as clicking that session in the
   * drawer. Absent when the caller offers no navigation, in which case a row's status chip stays
   * plain text rather than becoming a control that goes nowhere.
   */
  onOpenSession?: (sessionId: string) => void;
  /**
   * Every session in this orchestrator's stack, on any host, assembled by `workItemSessions` from
   * the session list plus the participant metadata the drawer parses. Each row resolves its own child
   * out of it — the only join that crosses the host boundary, since `QueryBranch`'s session leg reads
   * one daemon's sessions directory (D38, D39).
   */
  childSessions?: StackChildSession[];
  /** This stack's orchestrator session id — the other half of every child's identity (D39). */
  orchestratorSessionId?: string;
  /**
   * The poll set `branchResolutionByBranch` answers, from `buildBranchQueries` — needed because a
   * resolution alone does not say which **question** was asked for it.
   *
   * A row may read a `pr` leg off a planned name only when the query under that name was itself a
   * `planned-name` (D41). One name is queried once, and an owned branch wins it, so a branchless node
   * whose suggestion collides with a branch another node created would otherwise render that node's
   * PR as its own.
   */
  branchQueries?: BranchQuery[];
  /**
   * Brackets drawn on the right gutter between linked tasks — PR-stack line edges (`kind: "line"`,
   * a child's PR base is the parent's branch) and stored dependencies (`kind: "dependency"`).
   */
  connectors?: ConnectorEdge[];
  /**
   * Move a **free** task (no branch or PR, on no line) by `delta` rows — negative is up. Absent: no
   * task can be dragged. A task bound to a PR stack never calls it; its handle is disabled.
   */
  onMove?: (itemId: string, delta: number) => void;
}

/**
 * Renders one row per planned `WorkItem`, in the order the plan persists (see `orderWorkItems`).
 *
 * The list owns which rows are expanded, rather than each row holding its own state, so the set is a
 * property of the list that survives every re-render a poll tick causes.
 */
export function WorkItemList({
  nodes,
  onStartSession,
  startingNodeId,
  sessions = [],
  branchResolutionByBranch = {},
  onRepoint,
  defaultBranch = "",
  repointErrorByNodeId = {},
  branchMutatingNodeIds = new Set<string>(),
  onReorder,
  reorderErrorByNodeId = {},
  reorderingNodeIds = new Set<string>(),
  onSyncFromBase,
  syncErrorByNodeId = {},
  onOpenSession,
  childSessions = [],
  orchestratorSessionId = "",
  branchQueries = [],
  connectors = [],
  onMove,
}: WorkItemListProps) {
  const ordered = orderWorkItems(nodes);
  // The names the poll asked about as planned rather than owned — the discriminator a row's
  // `workItem` is gated on.
  const plannedNames = plannedNameBranches(branchQueries);
  const nodeById = new Map(nodes.map((n) => [n.itemId, n]));
  const [inner, setInner] = useState<HTMLDivElement | null>(null);
  const { drag, start: startDrag } = useItemDrag(
    inner,
    ordered.map((n) => n.itemId),
    (itemId, delta) => onMove?.(itemId, delta),
  );
  // A task on a line (either end of a line edge) belongs to its PR stack, as does one with a branch
  // or a PR: only a task with none of these is free to move.
  const onLine = new Set(connectors.filter((c) => c.kind === "line").flatMap((c) => [c.from, c.to]));
  const isBound = (node: WorkItem) =>
    Boolean(node.branch) || Boolean(node.pr) || Boolean(node.prStatus?.url) || onLine.has(node.itemId);
  const [expandedNodeIds, setExpandedNodeIds] = useState<ReadonlySet<string>>(new Set());

  const toggleExpanded = (itemId: string) => {
    setExpandedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(itemId)) {
        next.delete(itemId);
      } else {
        next.add(itemId);
      }
      return next;
    });
  };

  return (
    <div data-testid="work-item-list" className="overflow-y-auto p-3">
    <div
      ref={setInner}
      data-testid="work-item-stack"
      className="relative flex flex-col gap-3"
      style={{ paddingRight: CONNECTOR_GUTTER }}
    >
      {ordered.length === 0 ? (
        <p className="text-sm text-muted-foreground">No planned PRs yet.</p>
      ) : (
        ordered.map((node, index) => {
          // In progress when a live session owns the node's branch.
          const owner = resolveNodeSession(node, sessions);
          const inProgress = Boolean(owner?.isActive);
          const resolution = node.branch ? branchResolutionByBranch[node.branch] : undefined;
          // The child working this node anywhere in the fleet — the row's only cross-host signal.
          const childSession = nodeChildSession(node, childSessions, orchestratorSessionId);
          // The same join without its branch-ownership leg. Only a session that claims *this node* —
          // by node id, or by being the child the plan records — proves the node's own child still
          // exists, and that is the only evidence allowed to override the orphan verdict (D40). A
          // session that merely owns the branch now is a different session, and reading it as proof
          // would hide the exact D7 orphan the recovery CTA exists for.
          const identityChildSession = nodeChildSessionByIdentity(
            node,
            childSessions,
            orchestratorSessionId,
          );
          // A node that owns no branch is polled on its planned name, and **only** that resolution's
          // `pr` leg is read (D41): it is the one host-independent leg, and every other leg of it
          // describes a ref that does not exist. Taking the leg here rather than the whole resolution
          // is what keeps a planned name out of base resolution, the blockers and the branch line.
          //
          // Gated on the query's own kind, never on "this node has a suggestion and no branch": one
          // name is polled once and an owned branch wins it, so a suggestion colliding with a branch
          // another node created resolves to *that* node's ref — and its PR.
          const plannedName = node.branchSuggestion ?? "";
          const workItem = plannedNames.has(plannedName)
            ? branchResolutionByBranch[plannedName]?.pr
            : undefined;
          // Every reason this node cannot be started right now — see `startBlockers` for the rules.
          const blockers = startBlockers(node, nodes, branchResolutionByBranch);
          // Repoint is offered for the original merged-predecessor case *and* whenever the base
          // cannot be resolved right now, for any cause (D17). The plan's own `pr_status` is written
          // by the orchestrator agent and is stale in exactly the dead-end case — a merged
          // predecessor whose branch was deleted — so gating on it alone left that node unrecoverable.
          const canRepoint =
            blockers.length > 0 ||
            node.parents.some((parentId) => nodeById.get(parentId)?.prStatus?.phase === "merged");

          // The base branch as the row states it. `resolveWorkItemBase` distinguishes the three cases a
          // plain name cannot: a concrete ancestor ref, the project default (a root or an all-merged
          // chain), and a chain whose ancestors own no ref at all — which is named in words rather
          // than with the blocked ancestor's *planned* branch, since that branch does not exist and
          // would read as a base the child could be created from.
          const base = resolveWorkItemBase(node, nodes);
          const baseBranchLabel =
            base.kind === "ancestor-branch"
              ? base.branch
              : base.kind === "default-branch"
                ? // Empty for a legacy project with no stored `main_branch_ref` (D20).
                  defaultBranch || "default branch"
                : "no predecessor branch yet";

          return (
            <WorkItemRow
              key={node.itemId}
              node={node}
              onStartSession={onStartSession}
              starting={startingNodeId === node.itemId}
              inProgress={inProgress}
              resolution={resolution}
              canRepoint={canRepoint}
              onRepoint={onRepoint}
              repointTarget={resolveRepointTarget(
                node,
                nodes,
                branchResolutionByBranch,
                defaultBranch,
              )}
              repointError={repointErrorByNodeId[node.itemId]}
              branchMutating={branchMutatingNodeIds.has(node.itemId)}
              // The ends of the *rendered* order, which is the order the operator is looking at —
              // the persisted positions themselves may be sparse or start anywhere.
              canMoveUp={index > 0}
              canMoveDown={index < ordered.length - 1}
              onReorder={onReorder}
              reorderError={reorderErrorByNodeId[node.itemId]}
              reordering={reorderingNodeIds.has(node.itemId)}
              onSyncFromBase={onSyncFromBase}
              syncError={syncErrorByNodeId[node.itemId]}
              blockers={blockers}
              baseBranchLabel={baseBranchLabel}
              expanded={expandedNodeIds.has(node.itemId)}
              onToggleExpanded={toggleExpanded}
              parentTitles={parentTitles(node, nodes)}
              // A resolved child is a session the caller knows by construction, and it is the only
              // one of the two that can name a session on another host — so it takes precedence.
              // `boundChildSession` stays the fallback, keeping D23's ordering for a node no
              // participant claims.
              boundSessionId={
                childSession?.sessionId || boundChildSession(node, resolution, sessions)
              }
              onOpenSession={onOpenSession}
              workItem={workItem}
              childSession={childSession}
              identityChildSession={identityChildSession}
              dragHandle={
                <WorkItemDragHandle
                  itemId={node.itemId}
                  bound={isBound(node) || !onMove}
                  dragging={drag?.itemId === node.itemId}
                  onPointerDown={startDrag(node.itemId)}
                />
              }
            />
          );
        })
      )}
      {drag && (
        <div
          data-testid="work-item-drop-indicator"
          aria-hidden="true"
          style={{ position: "absolute", left: 0, right: CONNECTOR_GUTTER, top: drag.indicatorY - 1, height: 3, borderRadius: 2, background: "var(--wq-graph-dependency, #2563eb)", pointerEvents: "none" }}
        />
      )}
      {connectors.length > 0 && (
        <WorkItemConnectors
          container={inner}
          edges={connectors}
          layoutKey={ordered.map((n) => n.itemId).join(",") + [...expandedNodeIds].join(",")}
        />
      )}
    </div>
    </div>
  );
}
