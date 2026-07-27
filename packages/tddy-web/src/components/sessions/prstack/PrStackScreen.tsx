import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../../gen/connection_pb";
import type { SessionAttachmentState } from "../useSessionAttachment";
import { Button } from "../../ui/button";
import { detectIsMobile, useIsMobile } from "../../../hooks/useIsMobile";
import { usePresenterLiveKitRoom } from "../usePresenterLiveKitRoom";
import { PlannedPrList } from "./PlannedPrList";
import { PlannedPrPanel, PLANNED_PR_PANEL_WIDTH_PX } from "./PlannedPrPanel";
import { AddPlannedPrForm, type AddPlannedPrFormSubmission } from "./AddPlannedPrForm";
import { PrStackChat } from "./PrStackChat";
import { parseStackPlan, type StackNode } from "./stackPlan";
import { useQueryBranch } from "./useQueryBranch";
import { deriveStackBaseBranch, resolveStackBase } from "./deriveStackBaseBranch";
import { resolveRepointTarget } from "./startBlockers";
import { CreateSessionDialog } from "../CreateSessionDialog";
import type { CreateSessionInitialValues } from "../CreateSessionPane";

type ConnectionClient = Client<typeof ConnectionService>;

const IDLE_ATTACHMENT: SessionAttachmentState = { status: "idle" };

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
   * The session's own attach state. The chat panel derives its own independent LiveKit room
   * connection from this (see `usePresenterLiveKitRoom`) rather than being handed a room from
   * above — `SessionMainPane`'s `room` prop is VNC-purpose and unrelated.
   */
  attachment?: SessionAttachmentState;
  /**
   * The project's default branch (`ProjectEntry.main_branch_ref`, resolved by `session.projectId`).
   * Names the base in a root node's Start-session dialog and is the repoint target when no parent can
   * serve as a base. Empty for a legacy project that stores none — the label then reads "default
   * branch" and the daemon resolves the real ref when the repoint arrives (D20).
   */
  defaultBranch?: string;
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
  attachment = IDLE_ATTACHMENT,
  defaultBranch = "",
  onChildSessionStarted,
}: PrStackScreenProps) {
  const { room, status: roomStatus, error: roomError } = usePresenterLiveKitRoom(attachment);
  const livekitServerIdentity =
    attachment.status === "connected-livekit" ? attachment.livekitServerIdentity : undefined;
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
  const [startSessionNode, setStartSessionNode] = useState<StackNode | null>(null);
  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);
  // Why a node's last repoint did not happen, keyed by node id. Per node rather than one banner: the
  // list shows several nodes at once and only the one that was refused is still blocked.
  const [repointErrorByNodeId, setRepointErrorByNodeId] = useState<Record<string, string>>({});
  // Nodes with a repoint in flight, so a second click cannot start one beside it. Repointing a node
  // that owns a branch rebases and force-pushes it, which is not safe to run twice concurrently. A set
  // rather than a single id: repoints of different nodes are independent and may legitimately overlap.
  const [repointingNodeIds, setRepointingNodeIds] = useState<ReadonlySet<string>>(new Set());
  // The panel keeps today's at-a-glance view of the plan on desktop, where there is room for it, and
  // starts out of the way on mobile, where it covers the chat entirely (same seed as the session
  // list's own `sessionListOpen`).
  const [plannedPrPanelOpen, setPlannedPrPanelOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  // The branches nodes own. `QueryBranch` resolves each one's live GitHub PR itself, so the screen
  // makes no second PR lookup: `GetPrStatus` reaches the same authenticated `GET /pulls` on the
  // daemon, and polling both would double the GitHub request rate for no extra information — enough
  // to exhaust a 5000/hour user limit within the hour on a five-node stack, after which every row
  // reads "PR status unavailable" and stays there.
  const branches = useMemo(
    () => stack.nodes.map((n) => n.branch).filter((b): b is string => Boolean(b)),
    [stack.nodes],
  );
  // Branches to resolve through `QueryBranch`: the branches nodes own, plus every node's *base*
  // branch. A node's startability is a property of its base — its worktree is created from
  // `origin/<base>` — and an unspawned node owns no branch of its own, so without the bases in the
  // poll set the thing that decides startability is never resolved at all.
  const resolvedBranches = useMemo(
    () =>
      [
        ...new Set([
          ...branches,
          ...stack.nodes.flatMap((n) => {
            const base = resolveStackBase(n, stack.nodes);
            return base.kind === "ancestor-branch" ? [base.branch] : [];
          }),
        ]),
      ].sort(),
    [branches, stack.nodes],
  );
  // One-call branch resolution (worktree + in-progress session + remote + PR) per branch, polled on
  // the same interval and independent of the agent.
  const branchResolutionByBranch = useQueryBranch(
    client,
    sessionToken,
    session.sessionId,
    resolvedBranches,
  );

  // Opening "Start session" no longer spawns the child directly — it opens the shared creation
  // form pre-filled from the planned node, so the operator can review/adjust before spawning.
  const handleStartSession = (node: StackNode) => {
    // The dialog is only rendered when a daemon client is available; without one, opening it
    // would leave the row in its "starting" state with no dialog to clear it.
    if (!client) return;
    setStartSessionNode(node);
  };

  // Planned-PR sessions default to a Claude Code CLI session, stack-parented to this orchestrator so
  // the child's worktree chains onto its branch, and pre-fill the planned branch and title/description.
  //
  // A node that already owns a branch is *resumed* onto it rather than asked to create it: the branch,
  // its worktree and its remote ref all outlive the session that made them, so `new_branch_from_base`
  // would fail on "branch already exists" — which is exactly the node this recovery path is for.
  const ownedBranch = startSessionNode?.branch ?? "";
  const startSessionInitialValues: CreateSessionInitialValues | undefined = startSessionNode
    ? {
        projectId: session.projectId,
        daemonInstanceId: session.daemonInstanceId,
        stackParent: session.sessionId,
        sessionType: "claude-cli",
        branchIntent: ownedBranch ? "work_on_selected_branch" : "new_branch_from_base",
        selectedBranch: ownedBranch,
        // The planned branch (feature/<stack>/<node>, pre-filled by the pr-stack agent) — only
        // meaningful when a branch still has to be created.
        newBranchName: ownedBranch ? "" : (startSessionNode.branchSuggestion ?? ""),
        // The concrete base branch the child will branch from — the node's nearest non-merged
        // ancestor's branch (predecessor stack branch), collapsing to the project's default branch
        // for a root.
        baseBranchLabel: deriveStackBaseBranch(startSessionNode, stack.nodes, defaultBranch),
        initialPrompt: [startSessionNode.title, startSessionNode.description]
          .filter(Boolean)
          .join("\n\n"),
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
    const node = stack.nodes.find((n) => n.nodeId === nodeId);
    if (!node) return;
    // The control is disabled while a repoint is in flight, but a second call must be impossible
    // rather than merely hard to trigger: a repeat rebase and force-push of the same branch is not a
    // repeat of a harmless read.
    if (repointingNodeIds.has(nodeId)) return;
    setRepointingNodeIds((prev) => new Set(prev).add(nodeId));
    // A new attempt clears the previous reason: keeping it beside a repoint that is in flight would
    // report a failure that is no longer the current state.
    setRepointErrorByNodeId((prev) => {
      if (!(nodeId in prev)) return prev;
      const next = { ...prev };
      delete next[nodeId];
      return next;
    });
    try {
      const res = await client.repointPlannedPr({
        sessionToken,
        sessionId: session.sessionId,
        nodeId,
        targetBaseBranch: resolveRepointTarget(
          node,
          stack.nodes,
          branchResolutionByBranch,
          defaultBranch,
        ),
      });
      setStackPlanOverride(res.stackPlanJson);
    } catch (err) {
      // The daemon refuses a target that names neither the default branch nor any parent's branch, and
      // the repoint can still fail on an unresolvable default branch or a rebase conflict. Nothing was
      // persisted in either case, so the row stays blocked and has to say why.
      setRepointErrorByNodeId((errors) => ({
        ...errors,
        [nodeId]: err instanceof Error ? err.message : String(err),
      }));
    } finally {
      setRepointingNodeIds((prev) => {
        const next = new Set(prev);
        next.delete(nodeId);
        return next;
      });
    }
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
              nodes={stack.nodes}
              onSubmit={handleAddPlannedPr}
              onCancel={() => setIsAddingPlannedPr(false)}
            />
          )}
          <PlannedPrList
            nodes={stack.nodes}
            onStartSession={handleStartSession}
            startingNodeId={startSessionNode?.nodeId ?? null}
            sessions={sessions}
            branchResolutionByBranch={branchResolutionByBranch}
            onRepoint={handleRepoint}
            defaultBranch={defaultBranch}
            repointErrorByNodeId={repointErrorByNodeId}
            repointingNodeIds={repointingNodeIds}
          />
        </PlannedPrPanel>
      </div>
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
