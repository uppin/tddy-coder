import { branchlessNonMergedParent, resolveStackBase } from "./deriveStackBaseBranch";
import type { StackNode } from "./stackPlan";

/**
 * The only part of a branch's `QueryBranch` resolution that startability depends on: whether
 * `origin/<branch>` exists, since a child worktree is fetched from it.
 *
 * Deliberately structural rather than the generated `BranchResolution`. The real message is
 * assignable to it, so callers pass their resolution map unchanged, while a test can state the one
 * fact that matters without constructing a protobuf message.
 */
export interface BranchRemoteState {
  remote?: { exists: boolean };
}

/**
 * One reason a planned node cannot be started right now, carrying the text the row shows the
 * operator. Each kind also carries the subject it is about, so a caller can act on it without
 * re-parsing the message.
 */
export type StartBlocker =
  /** The base branch exists in the plan but not on `origin`, so the child's worktree cannot be fetched. */
  | { kind: "base-branch-not-on-origin"; branch: string; message: string }
  /** A direct parent has neither merged nor created a branch — the daemon's own spawn gate. */
  | { kind: "parent-has-no-branch"; parentTitle: string; message: string }
  /** No ancestor at all owns a created branch, so there is no ref anywhere in the chain to base onto. */
  | { kind: "no-ancestor-branch"; message: string };

/**
 * Every reason `node` cannot be started right now, in the order the row should read them. An empty
 * array means startable.
 *
 * This replaces a boolean pair (`baseBranchMissing` + one branch name), which could express a single
 * reason and no text — so a row blocked for two reasons named only one of them, and the operator was
 * left to infer the rest.
 *
 * Ordering is deliberate: `parent-has-no-branch` first. It is the blocker with an action behind it
 * (start that predecessor), whereas an absent `origin` ref is a state the operator recovers from by
 * repointing.
 *
 * Suppression rules, each of which exists because ignoring it produced a dead end:
 *
 * - **A node that owns a branch has no blockers.** Its spawn *resumes* that branch
 *   (`work_on_selected_branch`): nothing is created and no chain base is resolved, so nothing about
 *   the base can make it fail. Gating such a row would leave an orphan whose predecessor never pushed
 *   with no way back.
 * - **An unarrived resolution is *unknown*, never missing.** `useQueryBranch` swallows failed polls,
 *   so reading an absent `remote` leg as "absent from origin" would block permanently on a poll that
 *   simply has not answered.
 * - **`no-ancestor-branch` is reported only when no direct parent is the blocker.** A branchless
 *   non-merged direct parent already makes the base `no-ancestor-branch`, so reporting both states one
 *   fact twice. The blocker therefore surfaces only when the block is *above* a merged parent — the
 *   one case `parent-has-no-branch` cannot express.
 * - **A dangling parent id is not a blocker.** A plan referencing a node that does not exist is
 *   malformed, not an unmet dependency, and the daemon's gate likewise refuses only on a parent it can
 *   resolve. {@link branchlessNonMergedParent} and {@link resolveStackBase} both take that stance
 *   already; so does a `parents` cycle, which they terminate on — this runs on every render, so a bad
 *   `stackPlanJson` must not be able to hang the screen.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR (D16).
 */
export function startBlockers(
  node: StackNode,
  nodes: StackNode[],
  branchResolutionByBranch: Record<string, BranchRemoteState>,
): StartBlocker[] {
  if (node.branch) return [];

  const blockers: StartBlocker[] = [];

  // The daemon's own spawn gate (`Stack::base_ref_for_spawn`) refuses on *any* non-merged branchless
  // direct parent, even beside a sibling parent that owns a good branch.
  const blockingParent = branchlessNonMergedParent(node, nodes);
  if (blockingParent !== null) {
    blockers.push({
      kind: "parent-has-no-branch",
      parentTitle: blockingParent.title,
      message: `${blockingParent.title} has not created its branch yet`,
    });
  }

  const base = resolveStackBase(node, nodes);
  if (base.kind === "no-ancestor-branch" && blockingParent === null) {
    blockers.push({ kind: "no-ancestor-branch", message: "No predecessor owns a branch yet" });
  }
  if (
    base.kind === "ancestor-branch" &&
    branchResolutionByBranch[base.branch]?.remote?.exists === false
  ) {
    blockers.push({
      kind: "base-branch-not-on-origin",
      branch: base.branch,
      message: `Base branch ${base.branch} is not on origin`,
    });
  }

  return blockers;
}

/**
 * The branch a repoint of `node` would land it on — the label of its "Repoint to `<target>`" control
 * and the `target_base_branch` sent with the click, so the daemon does exactly what the label
 * promised.
 *
 * The first direct parent that can serve as a base right now wins: not merged, owns a `branch`, and
 * that branch's `origin` ref is not known to be absent. When none qualifies the base collapses to
 * `defaultBranch` — which is the web-side statement of the daemon's retain rule (D18): it retains
 * exactly the parents that own the target, so a target no parent owns means every parent is dropped.
 *
 * Only **direct** parents are considered, matching the daemon: the retain rule can keep or drop the
 * node's own parent edges and nothing else, so an ancestor further up could never become this node's
 * base without an edge that the repoint does not create.
 *
 * `defaultBranch` may be empty — a legacy project stores no `main_branch_ref` (D20). That degrades
 * the *label* only: the daemon resolves the real default ref when the empty target arrives.
 */
export function resolveRepointTarget(
  node: StackNode,
  nodes: StackNode[],
  branchResolutionByBranch: Record<string, BranchRemoteState>,
  defaultBranch: string,
): string {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  for (const parentId of node.parents) {
    const parent = byId.get(parentId);
    if (!parent) continue;
    // A merged parent is dropped even when its branch is still on `origin` — that is the original
    // repoint: the merged work is in the base already.
    if (parent.prStatus?.phase === "merged") continue;
    if (!parent.branch) continue;
    // An unarrived resolution is unknown, not absent (see `startBlockers`), so only a resolution that
    // came back reporting no `origin` ref disqualifies a parent.
    if (branchResolutionByBranch[parent.branch]?.remote?.exists === false) continue;
    return parent.branch;
  }
  return defaultBranch;
}
