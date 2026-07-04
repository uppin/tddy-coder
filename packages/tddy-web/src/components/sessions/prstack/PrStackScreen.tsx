import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../../gen/connection_pb";
import type { SessionAttachmentState } from "../useSessionAttachment";
import { Button } from "../../ui/button";
import { usePresenterLiveKitRoom } from "./usePresenterLiveKitRoom";
import { PlannedPrList } from "./PlannedPrList";
import { AddPlannedPrForm, type AddPlannedPrFormSubmission } from "./AddPlannedPrForm";
import { PrStackChat } from "./PrStackChat";
import { parseStackPlan, type StackNode } from "./stackPlan";
import { CreateSessionDialog } from "../CreateSessionDialog";
import type { CreateSessionInitialValues } from "../CreateSessionPane";

type ConnectionClient = Client<typeof ConnectionService>;

const IDLE_ATTACHMENT: SessionAttachmentState = { status: "idle" };

export interface PrStackScreenProps {
  session: SessionEntry;
  client?: ConnectionClient;
  sessionToken?: string;
  /**
   * The session's own attach state. The chat panel derives its own independent LiveKit room
   * connection from this (see `usePresenterLiveKitRoom`) rather than being handed a room from
   * above — `SessionMainPane`'s `room` prop is VNC-purpose and unrelated.
   */
  attachment?: SessionAttachmentState;
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
 * sessions. Two panes: the planned-PR list (left) and a chat window backed by the session's
 * remote Presenter (right).
 */
export function PrStackScreen({
  session,
  client,
  sessionToken = "",
  attachment = IDLE_ATTACHMENT,
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
  const startSessionInitialValues: CreateSessionInitialValues | undefined = startSessionNode
    ? {
        projectId: session.projectId,
        daemonInstanceId: session.daemonInstanceId,
        stackParent: session.sessionId,
        sessionType: "claude-cli",
        branchIntent: "new_branch_from_base",
        // The planned branch (feature/<stack>/<node>, pre-filled by the pr-stack agent). Prefer an
        // already-created branch, else the plan's suggestion.
        newBranchName: startSessionNode.branch ?? startSessionNode.branchSuggestion ?? "",
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

  return (
    <div data-testid="pr-stack-screen" className="flex-1 min-h-0 flex overflow-hidden">
      <div className="w-1/2 min-w-0 border-r border-border flex flex-col overflow-hidden">
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
        />
      </div>
      <div className="w-1/2 min-w-0 flex flex-col overflow-hidden">
        <PrStackChat
          session={session}
          room={room}
          livekitServerIdentity={livekitServerIdentity}
          roomStatus={roomStatus}
          roomError={roomError}
        />
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
