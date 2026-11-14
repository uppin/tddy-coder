import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionService, SessionEntry } from "tddy-client-web/gen/session_pb";
import type { ProjectService } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import type { SessionAttachmentHint } from "tddy-client-web/connections/session";
import { useMediaPort } from "tddy-client-web";
import { Button, detectIsMobile, useIsMobile } from "tddy-ui-web";
import { PlannedPrList } from "./PlannedPrList";
import { PlannedPrPanel, PLANNED_PR_PANEL_WIDTH_PX } from "./PlannedPrPanel";
import { AddPlannedPrForm, type AddPlannedPrFormSubmission } from "./AddPlannedPrForm";
import { PrStackChat } from "./PrStackChat";
import { parseStackPlan, type StackNode } from "./stackPlan";
import { hydrateStackNodes } from "./hydrateStackNodes";
import { stackChildSessions, type StackChildMetadata } from "./stackChildSessions";
import { useQueryBranch } from "./useQueryBranch";
import { buildBranchQueries } from "./branchQueries";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { DirtyWorktreeDialog, type DirtyWorktreePrompt } from "./DirtyWorktreeDialog";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";
import { resolveRepointTarget, startBlockers } from "./startBlockers";
import { stackDocAttachments } from "./stackDocAttachments";
import { usePrStackStartSession } from "./usePrStackStartSession";
import { usePrStackPlan } from "./usePrStackPlan";
import { usePrStackBranchMutations } from "./usePrStackBranchMutations";
import { PrStackStartSessionDialog } from "./PrStackStartSessionDialog";

type ConnectionClient = Client<typeof SessionService>;
type ProjectClient = Client<typeof ProjectService>;
type CatalogClient = Client<typeof CatalogService>;
type PrStackClient = Client<typeof PrStackService>;
type SessionFilesClient = Client<typeof SessionFilesService>;
type WorktreeClient = Client<typeof WorktreeService>;
type TaskClient = Client<typeof TaskService>;

/**
 * The default for a caller that parses no participant metadata.
 *
 * Module-level rather than a `new Map()` in the parameter default: that default allocates a fresh
 * map on every render, and it is a dependency of the `childSessions` memo, which `nodes` depends on,
 * which the poll set, the base resolution and every row depend on in turn — so the whole chain would
 * recompute on every render for a caller that never has any metadata at all.
 */
const NO_SESSION_METADATA: ReadonlyMap<string, StackChildMetadata> = new Map();

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
  /** `session.SessionService` on the orchestrator's host — Start-session only. */
  client?: ConnectionClient;
  /** `project.ProjectService` on the same host — Start-session dialog project registry. */
  projectClient?: ProjectClient;
  /** `pr_stack.PrStackService` on the same host — planned-PR mutations and `QueryBranch`. */
  prStackClient?: PrStackClient;
  /** `catalog.CatalogService` on the same host — the Start-session dialog's catalog fan-out. */
  catalogClient?: CatalogClient;
  /** The session-files service on the same host as `client` — the Start-session dialog stages
   *  its attachments through it. */
  sessionFilesClient?: SessionFilesClient;
  /**
   * The worktree service on the same host as `client` — the Start-session dialog's host-document
   * picker browses worktrees through it. Absent for the same reason `client` can be: no daemon is
   * reachable yet, and the dialog is not offered at all without both.
   */
  worktreeClient?: WorktreeClient;
  /** The task service on the same host as `client`: the Start-session dialog cancels a start through it. */
  taskClient?: TaskClient;
  sessionToken?: string;
  /**
   * The full session list (all hosts). Used to resolve each planned node's in-progress child
   * session by branch (`node.branch === session.branch`) — see `resolveNodeSession`.
   */
  sessions?: SessionEntry[];
  /**
   * How the attached session is reached. The chat panel derives its own independent LiveKit room
   * connection from this (see `usePresenterRoom`) rather than being handed a room from
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
  sessionMetadataBySessionId?: ReadonlyMap<string, StackChildMetadata>;
}

/**
 * The PR-Stack Chat Screen — rendered in place of the terminal for `recipe === "pr-stack"`
 * sessions. A chat window backed by the session's remote Presenter owns the width, with the
 * planned-PR list in a dismissible panel on the right (see {@link PlannedPrPanel}).
 */
export function PrStackScreen({
  session,
  client,
  projectClient,
  prStackClient,
  catalogClient,
  sessionFilesClient,
  worktreeClient,
  taskClient,
  sessionToken = "",
  sessions = [],
  attachmentHint = null,
  defaultBranch = "",
  defaultRemote = "",
  onChildSessionStarted,
  onOpenSession,
  sessionMetadataBySessionId = NO_SESSION_METADATA,
}: PrStackScreenProps) {
  const { room, status: roomStatus, error: roomError } = useMediaPort().usePresenterRoom(attachmentHint);
  const livekitServerIdentity = attachmentHint?.serverIdentity;
  const {
    nodes,
    childSessions,
    stackTargetFields,
    branchResolutionByBranch,
    branchQueries,
    setResolution,
    setStackPlanOverride,
  } = usePrStackPlan({
    session,
    prStackClient,
    sessionToken,
    sessions,
    sessionMetadataBySessionId,
    defaultBranch,
  });

  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);
  // The panel keeps today's at-a-glance view of the plan on desktop, where there is room for it, and
  // starts out of the way on mobile, where it covers the chat entirely (same seed as the session
  // list's own `sessionListOpen`).
  const [plannedPrPanelOpen, setPlannedPrPanelOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const {
    startSessionNode,
    setStartSessionNode,
    handleStartSession,
    startSessionInitialValues,
    handleChildSessionCreated,
  } = usePrStackStartSession({
    session,
    nodes,
    defaultBranch,
    defaultRemote,
    branchResolutionByBranch,
    hasSessionClient: Boolean(client),
    onChildSessionStarted,
  });

  const handleAddPlannedPr = async (input: AddPlannedPrFormSubmission) => {
    if (!prStackClient) return;
    const res = await prStackClient.addPlannedPr({
      sessionToken,
      ...stackTargetFields,
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
  const {
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
  } = usePrStackBranchMutations({
    prStackClient,
    sessionToken,
    stackTargetFields,
    nodes,
    defaultBranch,
    branchResolutionByBranch,
    setResolution,
    setStackPlanOverride,
  });

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
      <PrStackStartSessionDialog
        open={startSessionNode !== null}
        client={client}
        projectClient={projectClient}
        catalogClient={catalogClient}
        sessionFilesClient={sessionFilesClient}
        worktreeClient={worktreeClient}
        taskClient={taskClient}
        sessionToken={sessionToken}
        initialValues={startSessionInitialValues}
        onClose={() => setStartSessionNode(null)}
        onCreated={handleChildSessionCreated}
      />
    </div>
  );
}
