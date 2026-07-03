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
import { CLAUDE_CLI_MODELS } from "../../../constants/claudeCliModels";

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
  const [startingNodeId, setStartingNodeId] = useState<string | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);

  const handleStartSession = async (node: StackNode) => {
    if (!client) return;
    setStartingNodeId(node.nodeId);
    setStartError(null);
    try {
      // Planned-PR sessions default to a Claude Code CLI session for now. The daemon dispatches on
      // `sessionType === "claude-cli"` and ignores toolPath/agent/recipe for that path, so those
      // stay empty — but it *requires* a model and uses `permissionMode`. `stackParent` still chains
      // the child's worktree onto the orchestrator's branch (see start_claude_cli_session).
      const res = await client.startSession({
        sessionToken,
        projectId: session.projectId,
        toolPath: "",
        agent: "",
        recipe: "",
        stackParent: session.sessionId,
        sessionType: "claude-cli",
        model: CLAUDE_CLI_MODELS[0]?.id ?? "",
        permissionMode: "auto",
        initialPrompt: [node.title, node.description].filter(Boolean).join("\n\n"),
        sandbox: false,
        branchWorktreeIntent: "new_branch_from_base",
        // The planned branch (feature/<stack>/<node>, pre-filled by the pr-stack agent) — the daemon
        // requires a non-empty new_branch_name for new_branch_from_base. Prefer an already-created
        // branch, else the plan's suggestion.
        newBranchName: node.branch ?? node.branchSuggestion ?? "",
        selectedIntegrationBaseRef: "",
        selectedBranchToWorkOn: "",
        daemonInstanceId: session.daemonInstanceId,
      });
      onChildSessionStarted?.({
        sessionId: res.sessionId,
        recipe: node.childRecipe,
        orchestratorSessionId: session.sessionId,
        projectId: session.projectId,
      });
    } catch (err) {
      setStartError(err instanceof Error ? err.message : String(err));
    } finally {
      setStartingNodeId(null);
    }
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
        {startError && (
          <p className="text-xs text-destructive px-3 pt-2" role="alert">
            {startError}
          </p>
        )}
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
          startingNodeId={startingNodeId}
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
    </div>
  );
}
