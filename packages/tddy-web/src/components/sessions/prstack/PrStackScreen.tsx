import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../../gen/connection_pb";
import type { SessionAttachmentHint } from "../../../rpc/connections/session";
import { Button } from "../../ui/button";
import { detectIsMobile, useIsMobile } from "../../../hooks/useIsMobile";
import { usePresenterLiveKitRoom } from "../usePresenterLiveKitRoom";
import { PlannedPrList } from "./PlannedPrList";
import { PlannedPrPanel, PLANNED_PR_PANEL_WIDTH_PX } from "./PlannedPrPanel";
import { AddPlannedPrForm, type AddPlannedPrFormSubmission } from "./AddPlannedPrForm";
import { PrStackChat } from "./PrStackChat";
import { parseStackPlan, type StackNode } from "./stackPlan";
import { hydrateStackNodes } from "./hydrateStackNodes";
import { stackChildSessions } from "./stackChildSessions";
import { useQueryBranch } from "./useQueryBranch";
import { buildBranchQueries } from "./branchQueries";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { DirtyWorktreeDialog, type DirtyWorktreePrompt } from "./DirtyWorktreeDialog";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import { stackDocAttachments } from "./stackDocAttachments";
import { CreateSessionDialog } from "../CreateSessionDialog";
import type { CreateSessionInitialValues } from "../CreateSessionPane";
import { remoteTrackingName } from "../../../lib/branchNames";
import type { SessionMetadata } from "../../../lib/sessionParticipantMetadata";

type ConnectionClient = Client<typeof ConnectionService>;


/**
 * The default for a caller that parses no participant metadata.
 *
 * Module-level rather than a `new Map()` in the parameter default: that default allocates a fresh
 * map on every render, and it is a dependency of the `childSessions` memo, which `nodes` depends on,
 * which the poll set, the base resolution and every row depend on in turn — so the whole chain would
 * recompute on every render for a caller that never has any metadata at all.
 */
const NO_SESSION_METADATA: ReadonlyMap<string, SessionMetadata> = new Map();

/** The reason map without `nodeId`'s entry, or the map itself when it holds none. */
function withoutNode(errors: Record<string, string>, nodeId: string): Record<string, string> {
  if (!(nodeId in errors)) return errors;
  const next = { ...errors };
  delete next[nodeId];
  return next;
}

/** The in-flight set without `nodeId`, leaving the previous set untouched. */
function withoutNodeInFlight(inFlight: ReadonlySet<string>, nodeId: string): ReadonlySet<string> {
  const next = new Set(inFlight);
  next.delete(nodeId);
  return next;
}

/** The message an operator is shown for a rejected call, whatever the client threw. */
function failureReason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * What a pull that landed locally but never reached the remote tells the operator.
 *
 * The daemon answers such a pull with *success* and `pushed = false` (D32): the merge or rebase is in
 * the branch, and rolling it back would be strictly worse than reporting the truth. Reported as an
 * outright failure it would read as "nothing happened", and silently as success it would leave the
 * open PR still showing itself behind with the row claiming it is in sync — which is the exact state
 * D32 exists to make visible. So the wording states both halves: the work landed, the remote does not
 * have it.
 */
function unpushedPullReason(baseBranch: string, branch: string, pushError: string): string {
  return `Pulled ${baseBranch} into ${branch} locally, but the push failed — ${branch} on the remote (and its PR) does not have it yet: ${pushError}`;
}

export interface PrStackScreenProps {
  session: SessionEntry;
  client?: ConnectionClient;
  sessionToken?: string;
  /**
   * The full session list (all hosts). Used to resolve each planned node's in-progress child
   * session by branch (`node.branch === session.branch`) — see `resolveNodeSession`.
   */
  sessions?: SessionEntry[];
  /**
   * How the attached session is reached. The chat panel derives its own independent LiveKit room
   * connection from this (see `usePresenterLiveKitRoom`) rather than being handed a room from
   * above — `SessionMainPane`'s `room` prop is VNC-purpose and unrelated. `null` for a session that
   * is not attached, or one whose host serves it without a room.
   */
  attachmentHint?: SessionAttachmentHint | null;
  /**
   * The project's default branch (`ProjectEntry.main_branch_ref`, resolved by `session.projectId`).
   * Names the base in a root node's Start-session dialog and is the repoint target when no parent can
   * serve as a base. Empty for a legacy project that stores none — the label then reads "default
   * branch" and the daemon resolves the real ref when the repoint arrives (D20).
   */
  defaultBranch?: string;
  /**
   * The project's resolved default remote (`ProjectEntry.default_remote`, e.g. `origin`,
   * `upstream`). Prepended to the local branch names the Start-session dialog's "Base branch" picker
   * offers, so the value sent as `selected_integration_base_ref` is the `<remote>/<branch>` ref the
   * daemon fetches — not a bare local name whose first path segment it would mistake for a remote.
   * Empty for a legacy project that stored none; the view falls back to `origin` (the daemon's own
   * last resort).
   */
  defaultRemote?: string;
  /**
   * Fired after a child session is spawned so the caller can make it appear in the drawer.
   * Receives just enough of the new `SessionEntry` to render a drawer row immediately —
   * callers still refetch the full session list separately (`refreshSessions` in
   * `SessionsDrawerScreen`) for full field fidelity once the daemon's enrichment has run.
   */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
  /**
   * Select and attach an existing session — how a spawned planned PR's status chip opens the child
   * session it is bound to. Absent when the caller offers no navigation, in which case the chip stays
   * plain text rather than becoming a control that goes nowhere.
   */
  onOpenSession?: (sessionId: string) => void;
  /**
   * The `session` metadata block each live participant publishes, keyed by session id — the map the
   * drawer already keeps from common-room presence.
   *
   * It carries the one fact no `SessionEntry` does: which planned node a session materializes
   * (`stack_node_id`). Presence is also the only signal that crosses a host boundary — `ListSessions`
   * answers for one daemon's own sessions tree — so this is where a child running on another host
   * becomes joinable to the node it is working at all (D37, D38).
   */
  sessionMetadataBySessionId?: ReadonlyMap<string, SessionMetadata>;
}

/**
 * The PR-Stack Chat Screen — rendered in place of the terminal for `recipe === "pr-stack"`
 * sessions. A chat window backed by the session's remote Presenter owns the width, with the
 * planned-PR list in a dismissible panel on the right (see {@link PlannedPrPanel}).
 */
export function PrStackScreen({
  session,
  client,
  sessionToken = "",
  sessions = [],
  attachmentHint = null,
  defaultBranch = "",
  defaultRemote = "",
  onChildSessionStarted,
  onOpenSession,
  sessionMetadataBySessionId = NO_SESSION_METADATA,
}: PrStackScreenProps) {
  const { room, status: roomStatus, error: roomError } = usePresenterLiveKitRoom(attachmentHint);
  const livekitServerIdentity = attachmentHint?.serverIdentity;
  // Overrides `session.stackPlanJson` immediately after a successful `AddPlannedPr`, since the
  // `session` prop itself only refreshes once the caller separately refetches the session list.
  // Reset whenever the prop actually changes so a later real refetch isn't masked by a stale one.
  const [stackPlanOverride, setStackPlanOverride] = useState<string | null>(null);
  useEffect(() => {
    setStackPlanOverride(null);
  }, [session.stackPlanJson]);
  const stack = useMemo(
    () => parseStackPlan(stackPlanOverride ?? session.stackPlanJson),
    [stackPlanOverride, session.stackPlanJson],
  );
  // Every session in this stack, on any host, in the one shape the view joins on (D38).
  const childSessions = useMemo(
    () => stackChildSessions(sessions, sessionMetadataBySessionId),
    [sessions, sessionMetadataBySessionId],
  );
  // The plan as it stands, plus the `branch` and `session_id` a cross-host spawn wrote on the child's
  // own disk and this orchestrator's `changeset.yaml` therefore never learned. Hydrated once, here,
  // so `nodes` is what the whole screen reads: the poll set, base resolution, the spawn gate, the
  // parent picker and each row's branch line all become correct without a case of their own.
  const nodes = useMemo(
    () => hydrateStackNodes(stack.nodes, childSessions, session.sessionId),
    [stack.nodes, childSessions, session.sessionId],
  );
  const [startSessionNode, setStartSessionNode] = useState<StackNode | null>(null);
  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);
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
  const [plannedPrPanelOpen, setPlannedPrPanelOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  // What to ask `QueryBranch` to resolve: one call per branch a node owns, each paired with that
  // node's base so the daemon can also report how the two stand against each other. `QueryBranch`
  // resolves each branch's live GitHub PR itself, so the screen makes no second PR lookup:
  // `GetPrStatus` reaches the same authenticated `GET /pulls` on the daemon, and polling both would
  // double the GitHub request rate for no extra information — enough to exhaust a 5000/hour user
  // limit within the hour on a five-node stack, after which every row reads "PR status unavailable"
  // and stays there.
  const branchQueries = useMemo(
    () => buildBranchQueries(nodes, defaultBranch),
    [nodes, defaultBranch],
  );
  // One-call branch resolution (worktree + in-progress session + remote + PR + base sync) per branch,
  // polled on the same interval and independent of the agent.
  const { resolutionByBranch: branchResolutionByBranch, setResolution } = useQueryBranch(
    client,
    sessionToken,
    session.sessionId,
    branchQueries,
  );

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
        sessionType: "claude-cli",
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

  const handleAddPlannedPr = async (input: AddPlannedPrFormSubmission) => {
    if (!client) return;
    const res = await client.addPlannedPr({
      sessionToken,
      sessionId: session.sessionId,
      title: input.title,
      description: input.description,
      branchSuggestion: input.branchSuggestion,
      parents: input.parents,
      childRecipe: "",
    });
    setStackPlanOverride(res.stackPlanJson);
    if (input.startSession) {
      // The node the server says it created. Deliberately NOT the node this plan has and the one
      // held before the call did not: the orchestrator *agent* appends nodes to the same stack, and
      // `stack` only refreshes when the session list is refetched — so within one poll interval the
      // returned plan can hold several ids this screen has never seen, and any positional pick
      // (first, last) could open the dialog on the agent's node instead of the operator's.
      const added = parseStackPlan(res.stackPlanJson).nodes.find(
        (node) => node.nodeId === res.nodeId,
      );
      if (!added) {
        // The node the response named is absent from the plan the same response carried — the two
        // halves disagree, so there is nothing trustworthy to start. Reported through the form
        // (which stays open) rather than starting a session for a guessed node.
        throw new Error(
          "The added planned PR is missing from the returned stack plan — no session was started.",
        );
      }
      // The same path the row's own "Start session" CTA takes, so the dialog and its pre-filled
      // values are derived once.
      setStartSessionNode(added);
    }
    setIsAddingPlannedPr(false);
  };

  // Repoint retains exactly the parents that own the target branch, rebases the node's branch onto the
  // new effective base and re-targets the open PR — then re-renders the list from the returned stack
  // (same override mechanism as `handleAddPlannedPr`, since the `session` prop only refreshes on a
  // later refetch).
  //
  // The target is the same value the row's control named, so the daemon does exactly what the operator
  // was promised rather than re-deriving it from a git probe that cannot tell "absent from origin"
  // from "could not tell" (D18).
  const handleRepoint = async (nodeId: string) => {
    if (!client) return;
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
      const res = await client.repointPlannedPr({
        sessionToken,
        sessionId: session.sessionId,
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
    if (!client) return;
    // The control is disabled while a reorder is in flight, but a second call must be impossible
    // rather than merely hard to trigger: two reorders of one node race over which returned plan wins.
    if (reorderingNodeIds.has(nodeId)) return;
    setReorderingNodeIds((prev) => new Set(prev).add(nodeId));
    setReorderErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await client.reorderPlannedPr({
        sessionToken,
        sessionId: session.sessionId,
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
    if (!client) return;
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
      const res = await client.pullBaseIntoBranch({
        sessionToken,
        sessionId: session.sessionId,
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

  const plannedPrPanelState = plannedPrPanelOpen ? "open" : "closed";
  // Docked (desktop) the panel takes a column of its own; as a mobile overlay it covers the chat.
  const isPanelDocked = plannedPrPanelOpen && !isMobile;

  return (
    <div data-testid="pr-stack-screen" className="flex-1 min-h-0 flex flex-col overflow-hidden">
      <div className="flex-shrink-0 flex items-center justify-end border-b border-border px-2 py-1">
        <Button
          data-testid="pr-stack-planned-pr-panel-toggle"
          size="sm"
          variant="ghost"
          onClick={() => setPlannedPrPanelOpen((open) => !open)}
          title={plannedPrPanelOpen ? "Hide planned PRs" : "Show planned PRs"}
        >
          Planned PRs
        </Button>
      </div>
      {/* The panel is absolute within this row rather than the screen root, so it never covers the
          toggle above it. */}
      <div className="relative flex-1 min-h-0 overflow-hidden">
        {/* The chat owns the full width. A docked panel gets its own column beside it; the mobile
            overlay deliberately covers it instead. */}
        <div
          className="h-full flex flex-col overflow-hidden"
          style={{ paddingRight: isPanelDocked ? PLANNED_PR_PANEL_WIDTH_PX : undefined }}
        >
          <PrStackChat
            session={session}
            room={room}
            livekitServerIdentity={livekitServerIdentity}
            roomStatus={roomStatus}
            roomError={roomError}
          />
        </div>
        <PlannedPrPanel
          state={plannedPrPanelState}
          isMobile={isMobile}
          onClose={() => setPlannedPrPanelOpen(false)}
        >
          <div className="flex-shrink-0 flex justify-end p-3 pb-0">
            <Button
              data-testid="pr-stack-add-planned-pr-btn"
              size="sm"
              variant="outline"
              onClick={() => setIsAddingPlannedPr(true)}
            >
              + New planned PR
            </Button>
          </div>
          {isAddingPlannedPr && (
            <AddPlannedPrForm
              nodes={nodes}
              onSubmit={handleAddPlannedPr}
              onCancel={() => setIsAddingPlannedPr(false)}
            />
          )}
          <PlannedPrList
            nodes={nodes}
            onStartSession={handleStartSession}
            startingNodeId={startSessionNode?.nodeId ?? null}
            sessions={sessions}
            branchResolutionByBranch={branchResolutionByBranch}
            onRepoint={handleRepoint}
            defaultBranch={defaultBranch}
            repointErrorByNodeId={repointErrorByNodeId}
            branchMutatingNodeIds={branchMutatingNodeIds}
            onReorder={handleReorder}
            reorderErrorByNodeId={reorderErrorByNodeId}
            reorderingNodeIds={reorderingNodeIds}
            onSyncFromBase={handleSyncFromBase}
            syncErrorByNodeId={syncErrorByNodeId}
            onOpenSession={onOpenSession}
            childSessions={childSessions}
            orchestratorSessionId={session.sessionId}
            branchQueries={branchQueries}
          />
        </PlannedPrPanel>
      </div>
      <DirtyWorktreeDialog
        prompt={dirtyWorktreePrompt}
        onCommitAndPull={handleCommitDirtyWorktreeAndPull}
        onCancel={() => setDirtyWorktreePrompt(null)}
      />
      {client && (
        <CreateSessionDialog
          open={startSessionNode !== null}
          client={client}
          sessionToken={sessionToken}
          initialValues={startSessionInitialValues}
          onClose={() => setStartSessionNode(null)}
          onCreated={handleChildSessionCreated}
        />
      )}
    </div>
  );
}
