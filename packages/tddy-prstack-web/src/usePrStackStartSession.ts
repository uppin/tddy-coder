import { useState } from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { remoteTrackingName, type CreateSessionInitialValues } from "tddy-session-create-web";
import type { StackNode } from "./stackPlan";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";
import { startBlockers } from "./startBlockers";
import { stackDocAttachments } from "./stackDocAttachments";
import type { BranchResolution } from "tddy-client-web/gen/pr_stack_pb";

/**
 * The Start-session flow of the PR-stack screen (`#orchestrators` 12/12, B11): which planned node
 * the shared creation dialog is open for, the dialog's pre-filled initial values (the base-branch
 * derivation and every blocker that blanks its pre-selection), and what a created child reports
 * back. Split out of `PrStackScreen` as `usePrStackStartSession`.
 */
export function usePrStackStartSession(input: {
  session: SessionEntry;
  nodes: StackNode[];
  defaultBranch: string;
  defaultRemote: string;
  branchResolutionByBranch: Record<string, BranchResolution>;
  hasSessionClient: boolean;
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
}) {
  const { session, nodes, defaultBranch, defaultRemote, branchResolutionByBranch, hasSessionClient, onChildSessionStarted } = input;
  const client = hasSessionClient;
  const [startSessionNode, setStartSessionNode] = useState<StackNode | null>(null);

  // Opening "Start session" no longer spawns the child directly — it opens the shared creation
  // form pre-filled from the planned node, so the operator can review/adjust before spawning.
  const handleStartSession = (node: StackNode) => {
    // The dialog is only rendered when a daemon client is available; without one, opening it
    // would leave the row in its "starting" state with no dialog to clear it.
    if (!client) return;
    setStartSessionNode(node);
  };

  // The daemon's `selected_integration_base_ref` is a remote-tracking ref (`<remote>/<branch>`), but a
  // stack node's `branch` and the base-branch picker's options are local names. Lift the local names
  // into the form the daemon fetches (`git fetch <remote> <branch>`), using the project's resolved
  // default remote — falling back to `origin` (the daemon's own last resort) for a legacy project that
  // stored none. `remoteTrackingName` is idempotent, so a `defaultBranch` that is already
  // `<remote>/<branch>` (e.g. `origin/master`) passes through unchanged.
  const remote = defaultRemote || "origin";

  // Planned-PR sessions default to a Claude Code CLI session, stack-parented to this orchestrator so
  // the child's worktree chains onto its branch, and pre-fill the planned branch and title/description.
  //
  // A node that already owns a branch is *resumed* onto it rather than asked to create it: the branch,
  // its worktree and its remote ref all outlive the session that made them, so `new_branch_from_base`
  // would fail on "branch already exists" — which is exactly the node this recovery path is for.
  const ownedBranch = startSessionNode?.branch ?? "";
  // Options and pre-selection come from one resolver, so the picker cannot contradict the base label
  // below — both read `deriveStackBaseBranch`. Each option's ref is lifted into remote-tracking form,
  // and with it any label that IS that branch. A label that differs is prose, not a ref — today only a
  // legacy project's "project default", which names an empty ref in words — so it is left as written.
  const choice = startSessionNode
    ? baseBranchChoice(startSessionNode, nodes, defaultBranch)
    : { options: [], selected: "" };
  const baseBranchOptions = choice.options.map((option) => {
    const ref = remoteTrackingName(option.value, remote);
    return { value: ref, label: option.label === option.value ? ref : option.label };
  });
  // Every reason this spawn may not succeed, as the row states them.
  const startBlockersForNode = startSessionNode
    ? startBlockers(startSessionNode, nodes, branchResolutionByBranch)
    : [];
  // A blocked node sends **no** explicit base, so the daemon resolves the chain base itself.
  //
  // `selected_integration_base_ref` is an override: `select_worktree_base_ref` gives it precedence
  // over `resolve_chain_base_ref`, which is where `Stack::base_ref_for_spawn` — the daemon's own
  // ordering gate — lives. And the value it would be given here is derived from a base the view could
  // not resolve: `deriveStackBaseBranch` flattens an unresolvable base (`no-ancestor-branch`,
  // `parent-has-no-branch`) to the project default. Sending it would cut the child's worktree from
  // `origin/<default>` for a node whose non-merged parent owns no branch, record that branch on the
  // node, and leave every descendant based onto a branch missing its parent's work — silently, with a
  // healthy status chip on the row.
  //
  // Force start hands the decision to the daemon's gate (D42), so the view must not pre-empt that gate
  // with an override. The picker keeps every option: a base the operator chooses on purpose is a
  // decision, not a guess — only the pre-selection is dropped.
  // A blocked node's pre-selection is dropped only when there is more than one option to choose
  // from: the operator must then pick a base on purpose, and submitting without picking sends an
  // empty override so the daemon's own ordering gate decides (D42). When the project default is the
  // sole option — a child of a parent merged externally, whose branch is gone and whose work is
  // already in the default — the default is the escape and is pre-selected, so confirming sends it
  // as the explicit repoint the daemon honors past the gate. The caption always names the derived
  // base (D43: the dialog "showing the base branch"), never blanked.
  const blankBaseBranchSelection =
    startBlockersForNode.length > 0 && baseBranchOptions.length !== 1;
  const selectedBaseBranch = blankBaseBranchSelection
    ? ""
    : remoteTrackingName(choice.selected, remote);
  const startSessionInitialValues: CreateSessionInitialValues | undefined = startSessionNode
    ? {
        projectId: session.projectId,
        daemonInstanceId: session.daemonInstanceId,
        stackParent: session.sessionId,
        // The planned node this spawn materializes, sent so the daemon links it by identity rather
        // than re-deriving it from the branch (D34) — a branch the operator can still rename in the
        // dialog before confirming, and one the spawning daemon cannot look up at all when the
        // orchestrator lives on another host.
        stackNodeId: startSessionNode.nodeId,
        // The child runs as the same kind of session as its orchestrator parent.
        sessionType: session.sessionType,
        branchIntent: ownedBranch ? "work_on_selected_branch" : "new_branch_from_base",
        selectedBranch: ownedBranch,
        // The planned branch (feature/<stack>/<node>, pre-filled by the pr-stack agent) — only
        // meaningful when a branch still has to be created.
        newBranchName: ownedBranch ? "" : (startSessionNode.branchSuggestion ?? ""),
        // The concrete base branch the child will branch from — the node's nearest non-merged
        // ancestor's branch (predecessor stack branch), collapsing to the project's default branch
        // for a root. Lifted to `<remote>/<branch>` so the label matches the picker's options and
        // reads the same ref the daemon will fetch.
        //
        // Blank for a blocked node, for the same reason `selectedBaseBranch` is: the base that
        // `deriveStackBaseBranch` flattens to is a guess this spawn no longer submits, and a dialog
        // naming a ref it will not send is the drift D18 exists to prevent. The option then reads
        // "New branch from base" with no ref, which is the truth — the daemon resolves it, and
        // refuses the spawn when the chain has no base to give.
        baseBranchLabel:
          remoteTrackingName(deriveStackBaseBranch(startSessionNode, nodes, defaultBranch), remote),
        baseBranchOptions,
        selectedBaseBranch,
        // A stack node's branch is the base its successors' PRs target, so it goes to origin at
        // start — unlike an ad-hoc session, where the toggle defaults to unchecked.
        createRemoteBranch: true,
        initialPrompt: [startSessionNode.title, startSessionNode.description]
          .filter(Boolean)
          .join("\n\n"),
        // The orchestrator's documents, attached by reference so the child agent reads its own
        // boundaries rather than inferring them. Rows, not an invariant: an operator restarting an
        // orphaned node whose child already holds them drops them here.
        attachments: stackDocAttachments(session, startSessionNode.nodeId),
      }
    : undefined;

  const handleChildSessionCreated = (sessionId: string) => {
    const node = startSessionNode;
    setStartSessionNode(null);
    if (!node) return;
    onChildSessionStarted?.({
      sessionId,
      recipe: node.childRecipe,
      orchestratorSessionId: session.sessionId,
      projectId: session.projectId,
    });
  };


  return {
    startSessionNode,
    setStartSessionNode,
    handleStartSession,
    startSessionInitialValues,
    handleChildSessionCreated,
  };
}
