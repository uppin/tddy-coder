import React, { useState } from "react";
import type { BranchResolution, SessionEntry } from "../../../gen/connection_pb";
import { resolveNodeSession } from "../../../utils/resolveNodeSession";
import { PlannedPrRow } from "./PlannedPrRow";
import { boundChildSession } from "./boundChildSession";
import { plannedNameBranches, type BranchQuery } from "./branchQueries";
import { resolveStackBase } from "./deriveStackBaseBranch";
import { orderStackNodes } from "./orderStackNodes";
import { parentTitles } from "./parentTitles";
import { nodeChildSession, nodeChildSessionByIdentity } from "./nodeChildSession";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import type { StackChildSession } from "./stackChildSessions";
import type { StackNode } from "./stackPlan";

export interface PlannedPrListProps {
  nodes: StackNode[];
  onStartSession: (node: StackNode) => void;
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
  onRepoint?: (nodeId: string) => void;
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
  onReorder?: (nodeId: string, direction: "up" | "down") => void;
  /** The daemon's reason per node whose last reorder was refused or failed, keyed by node id. */
  reorderErrorByNodeId?: Record<string, string>;
  /** Nodes whose reorder is in flight — both of their controls are disabled until it settles. */
  reorderingNodeIds?: ReadonlySet<string>;
  /** Pull a node's base into its branch, by the strategy the clicked control named. */
  onSyncFromBase?: (nodeId: string, strategy: "merge" | "rebase") => void;
  /** The daemon's reason per node whose last pull was refused or failed, keyed by node id. */
  syncErrorByNodeId?: Record<string, string>;
  /**
   * Select and attach a node's bound child session — the same act as clicking that session in the
   * drawer. Absent when the caller offers no navigation, in which case a row's status chip stays
   * plain text rather than becoming a control that goes nowhere.
   */
  onOpenSession?: (sessionId: string) => void;
  /**
   * Every session in this orchestrator's stack, on any host, assembled by `stackChildSessions` from
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
}

/**
 * Renders one row per planned `StackNode`, in the order the plan persists (see `orderStackNodes`).
 *
 * The list owns which rows are expanded, rather than each row holding its own state, so the set is a
 * property of the list that survives every re-render a poll tick causes.
 */
export function PlannedPrList({
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
}: PlannedPrListProps) {
  const ordered = orderStackNodes(nodes);
  // The names the poll asked about as planned rather than owned — the discriminator a row's
  // `plannedPr` is gated on.
  const plannedNames = plannedNameBranches(branchQueries);
  const nodeById = new Map(nodes.map((n) => [n.nodeId, n]));
  const [expandedNodeIds, setExpandedNodeIds] = useState<ReadonlySet<string>>(new Set());

  const toggleExpanded = (nodeId: string) => {
    setExpandedNodeIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  };

  return (
    <div data-testid="pr-stack-planned-pr-list" className="flex flex-col gap-2 overflow-y-auto p-3">
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
          const plannedPr = plannedNames.has(plannedName)
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

          // The base branch as the row states it. `resolveStackBase` distinguishes the three cases a
          // plain name cannot: a concrete ancestor ref, the project default (a root or an all-merged
          // chain), and a chain whose ancestors own no ref at all — which is named in words rather
          // than with the blocked ancestor's *planned* branch, since that branch does not exist and
          // would read as a base the child could be created from.
          const base = resolveStackBase(node, nodes);
          const baseBranchLabel =
            base.kind === "ancestor-branch"
              ? base.branch
              : base.kind === "default-branch"
                ? // Empty for a legacy project with no stored `main_branch_ref` (D20).
                  defaultBranch || "default branch"
                : "no predecessor branch yet";

          return (
            <PlannedPrRow
              key={node.nodeId}
              node={node}
              onStartSession={onStartSession}
              starting={startingNodeId === node.nodeId}
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
              repointError={repointErrorByNodeId[node.nodeId]}
              branchMutating={branchMutatingNodeIds.has(node.nodeId)}
              // The ends of the *rendered* order, which is the order the operator is looking at —
              // the persisted positions themselves may be sparse or start anywhere.
              canMoveUp={index > 0}
              canMoveDown={index < ordered.length - 1}
              onReorder={onReorder}
              reorderError={reorderErrorByNodeId[node.nodeId]}
              reordering={reorderingNodeIds.has(node.nodeId)}
              onSyncFromBase={onSyncFromBase}
              syncError={syncErrorByNodeId[node.nodeId]}
              blockers={blockers}
              baseBranchLabel={baseBranchLabel}
              expanded={expandedNodeIds.has(node.nodeId)}
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
              plannedPr={plannedPr}
              childSession={childSession}
              identityChildSession={identityChildSession}
            />
          );
        })
      )}
    </div>
  );
}
