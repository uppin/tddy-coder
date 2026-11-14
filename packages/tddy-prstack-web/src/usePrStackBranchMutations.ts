import { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { PrStackService, BranchResolution } from "tddy-client-web/gen/pr_stack_pb";
import type { StackNode } from "./stackPlan";
import { resolveRepointTarget } from "./startBlockers";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { type DirtyWorktreePrompt } from "./DirtyWorktreeDialog";

/**
 * The PR-stack screen's branch mutations (`#orchestrators` 12/12, B11): repoint, reorder and
 * pull-from-base, with each node's in-flight set and refusal reason. Split out of
 * `PrStackScreen` so the screen composes the panel, not every mutation's bookkeeping.
 */

function withoutNode(errors: Record<string, string>, nodeId: string): Record<string, string> {
  if (!(nodeId in errors)) return errors;
  const next = { ...errors };
  delete next[nodeId];
  return next;
}

function withoutNodeInFlight(inFlight: ReadonlySet<string>, nodeId: string): ReadonlySet<string> {
  const next = new Set(inFlight);
  next.delete(nodeId);
  return next;
}

function failureReason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function unpushedPullReason(baseBranch: string, branch: string, pushError: string): string {
  return `Pulled ${baseBranch} into ${branch} locally, but the push failed — ${branch} on the remote (and its PR) does not have it yet: ${pushError}`;
}

export function usePrStackBranchMutations(input: {
  prStackClient?: Client<typeof PrStackService>;
  sessionToken: string;
  stackTargetFields: { stack: { projectId: string; stackId: string; daemonInstanceId: string } } | { sessionId: string };
  nodes: StackNode[];
  defaultBranch: string;
  branchResolutionByBranch: Record<string, BranchResolution>;
  setResolution: (branch: string, resolution: BranchResolution) => void;
  setStackPlanOverride: (json: string) => void;
}) {
  const { prStackClient, sessionToken, stackTargetFields, nodes, defaultBranch, branchResolutionByBranch, setResolution, setStackPlanOverride } = input;


  // Why a node's last repoint did not happen, keyed by node id. Per node rather than one banner: the
  // list shows several nodes at once and only the one that was refused is still blocked.
  const [repointErrorByNodeId, setRepointErrorByNodeId] = useState<Record<string, string>>({});
  // Nodes with a mutation of their branch in flight — a repoint *or* a pull from the base. One set
  // covering both, because both rewrite the same branch: a repoint rebases and force-pushes it, and a
  // pull merges or rebases the base into it. The daemon serializes neither, so the two running side by
  // side leaves a half-rebased worktree or force-pushes over a merge commit — and the state where both
  // controls are offered at once is the normal post-merge one, since a node becomes repointable
  // exactly when the parent whose merge also left it behind its new base landed.
  //
  // A set rather than a single id: mutations of *different* nodes touch different branches and may
  // legitimately overlap. Only the guard is shared — each operation still reports its own failure.
  const [branchMutatingNodeIds, setBranchMutatingNodeIds] = useState<ReadonlySet<string>>(new Set());
  // Why a node's last reorder did not happen. Nothing moved, so the row would otherwise look as
  // though the click was simply ignored.
  const [reorderErrorByNodeId, setReorderErrorByNodeId] = useState<Record<string, string>>({});
  // Nodes with a reorder in flight — the plan is rewritten from the response, so a second click
  // would race the first for which returned order wins.
  const [reorderingNodeIds, setReorderingNodeIds] = useState<ReadonlySet<string>>(new Set());
  // Why a node's last pull from its base did not happen, keyed by node id. Kept apart from the repoint
  // reasons: the two operations fail for different causes and the row states each where it happened.
  const [syncErrorByNodeId, setSyncErrorByNodeId] = useState<Record<string, string>>({});
  // The pull the operator asked for that is waiting on what to do about uncommitted work in the
  // node's worktree. Null when nothing is pending — the prompt is the only thing standing between
  // the click and the call.
  const [dirtyWorktreePrompt, setDirtyWorktreePrompt] = useState<DirtyWorktreePrompt | null>(null);
  // The panel keeps today's at-a-glance view of the plan on desktop, where there is room for it, and
  // starts out of the way on mobile, where it covers the chat entirely (same seed as the session
  // list's own `sessionListOpen`).

  const handleRepoint = async (nodeId: string) => {
    if (!prStackClient) return;
    const node = nodes.find((n) => n.nodeId === nodeId);
    if (!node) return;
    // The control is disabled while any mutation of this branch is in flight, but a second call must
    // be impossible rather than merely hard to trigger: a rebase and force-push landing beside a pull
    // that is merging into the same branch is not a repeat of a harmless read.
    if (branchMutatingNodeIds.has(nodeId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(nodeId));
    // A new attempt clears the previous reason: keeping it beside a repoint that is in flight would
    // report a failure that is no longer the current state.
    setRepointErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.repointPlannedPr({
        sessionToken,
        ...stackTargetFields,
        nodeId,
        targetBaseBranch: resolveRepointTarget(
          node,
          nodes,
          branchResolutionByBranch,
          defaultBranch,
        ),
      });
      setStackPlanOverride(res.stackPlanJson);
    } catch (err) {
      // The daemon refuses a target that names neither the default branch nor any parent's branch, and
      // the repoint can still fail on an unresolvable default branch or a rebase conflict. Nothing was
      // persisted in either case, so the row stays blocked and has to say why.
      setRepointErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  // Move a row one position in the persisted reading order. The plan comes back renumbered and is
  // re-rendered through the same override `handleAddPlannedPr` uses, since the `session` prop only
  // refreshes on a later refetch.
  const handleReorder = async (nodeId: string, direction: "up" | "down") => {
    if (!prStackClient) return;
    // The control is disabled while a reorder is in flight, but a second call must be impossible
    // rather than merely hard to trigger: two reorders of one node race over which returned plan wins.
    if (reorderingNodeIds.has(nodeId)) return;
    setReorderingNodeIds((prev) => new Set(prev).add(nodeId));
    setReorderErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.reorderPlannedPr({
        sessionToken,
        ...stackTargetFields,
        nodeId,
        direction,
      });
      setStackPlanOverride(res.stackPlanJson);
    } catch (err) {
      // Nothing was persisted, so nothing moved — without a reason the row would look as though the
      // click was simply swallowed.
      setReorderErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setReorderingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  // Take the base's commits into a node's branch. The base sent is the one the row's badge and the
  // control's own label named — the same discipline repoint follows (D18) — so the daemon does
  // exactly what the operator was promised rather than re-deriving a base of its own.
  const runPull = async (pull: {
    nodeId: string;
    /** The branch that takes the commits — the key the fresh resolution is written back under. */
    branch: string;
    baseBranch: string;
    strategy: "merge" | "rebase";
    /** "commit" commits and pushes the worktree's outstanding work first; empty leaves it to fail. */
    dirtyWorktreeAction: "" | "commit";
    commitMessage: string;
  }) => {
    if (!prStackClient) return;
    const { nodeId, branch } = pull;
    // Every control that touches this branch is disabled while one of them runs, but a concurrent
    // merge, rebase or repoint of one branch is destructive rather than merely wasteful, so it must
    // be impossible rather than merely unreachable through the UI.
    if (branchMutatingNodeIds.has(nodeId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(nodeId));
    // A new attempt clears the previous reason: a failure kept beside a pull that is now in flight
    // reports a state that is no longer true.
    setSyncErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.pullBaseIntoBranch({
        sessionToken,
        ...stackTargetFields,
        nodeId,
        baseBranch: pull.baseBranch,
        strategy: pull.strategy,
        dirtyWorktreeAction: pull.dirtyWorktreeAction,
        commitMessage: pull.commitMessage,
      });
      // The refs just moved, so the row repaints from the pull's own re-resolution instead of
      // waiting up to a full poll interval to stop claiming the branch is behind. Written into the
      // poll's own map rather than layered over it, so the next tick simply supersedes it.
      if (res.resolution) setResolution(branch, res.resolution);
      // A successful call is not necessarily a completed pull: the local merge or rebase can land
      // while the push that follows it fails, which the daemon reports rather than rolling back
      // (D32). The re-resolution above then repaints the row as in sync with the base — true of the
      // local branch, and not true of the PR anyone is reviewing — so the one surface that can say
      // so has to.
      if (!res.pushed && res.pushError) {
        setSyncErrorByNodeId((errors) => ({
          ...errors,
          [nodeId]: unpushedPullReason(pull.baseBranch, branch, res.pushError),
        }));
      }
    } catch (err) {
      // A conflict aborts the pull and a push can fail on its own; either way the row still reads
      // "behind", so it has to say why it stayed that way.
      setSyncErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  // The click on a merge or rebase control. A worktree holding uncommitted work is a prompt rather
  // than a refusal (D31): a child session's agent may be mid-turn in that checkout, so the operator
  // sees what is outstanding and chooses to commit it before anything is touched.
  const handleSyncFromBase = (nodeId: string, strategy: "merge" | "rebase") => {
    const node = nodes.find((n) => n.nodeId === nodeId);
    if (!node?.branch) return;
    // Refused here as well as in `runPull`, so a pull that cannot run never gets as far as opening
    // the dirty-worktree prompt — a prompt whose confirm button does nothing is its own dead end.
    if (branchMutatingNodeIds.has(nodeId)) return;
    const resolution = branchResolutionByBranch[node.branch];
    const view = baseSyncView(resolution);
    // The controls render only for a clean behind-count, and the base they name comes from the same
    // view — so a pull can never be issued against a comparison that was not made.
    if (!canPullFromBase(view)) return;
    if (resolution?.worktree?.dirty) {
      setDirtyWorktreePrompt({
        nodeId,
        branch: node.branch,
        baseBranch: view.baseBranch,
        strategy,
        dirtyPaths: resolution.worktree.dirtyPaths,
      });
      return;
    }
    void runPull({
      nodeId,
      branch: node.branch,
      baseBranch: view.baseBranch,
      strategy,
      dirtyWorktreeAction: "",
      commitMessage: "",
    });
  };

  // The operator confirmed the prompt: commit and push the outstanding work first, then pull with
  // the strategy they originally clicked.
  const handleCommitDirtyWorktreeAndPull = (commitMessage: string) => {
    const prompt = dirtyWorktreePrompt;
    if (!prompt) return;
    setDirtyWorktreePrompt(null);
    void runPull({
      nodeId: prompt.nodeId,
      branch: prompt.branch,
      baseBranch: prompt.baseBranch,
      // The strategy the operator originally clicked, not a reset to the default: confirming the
      // prompt answers what to do about the worktree, not which pull to run.
      strategy: prompt.strategy,
      dirtyWorktreeAction: "commit",
      commitMessage,
    });
  };


  return {
    repointErrorByNodeId,
    branchMutatingNodeIds,
    reorderErrorByNodeId,
    reorderingNodeIds,
    syncErrorByNodeId,
    dirtyWorktreePrompt,
    setDirtyWorktreePrompt,
    handleRepoint,
    handleReorder,
    handleSyncFromBase,
    handleCommitDirtyWorktreeAndPull,
  };
}
