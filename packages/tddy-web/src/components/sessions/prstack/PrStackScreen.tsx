import React, { useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { ConnectionService, SessionEntry } from "../../../gen/connection_pb";
import { PlannedPrList } from "./PlannedPrList";
import { PrStackChat } from "./PrStackChat";
import { parseStackPlan, type StackNode } from "./stackPlan";

type ConnectionClient = Client<typeof ConnectionService>;

export interface PrStackScreenProps {
  session: SessionEntry;
  client?: ConnectionClient;
  sessionToken?: string;
  /** LiveKit room for this orchestrator session, when attached over LiveKit. Null otherwise. */
  room?: Room | null;
  livekitServerIdentity?: string;
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
  room = null,
  livekitServerIdentity,
  onChildSessionStarted,
}: PrStackScreenProps) {
  const stack = useMemo(() => parseStackPlan(session.stackPlanJson), [session.stackPlanJson]);
  const [startingNodeId, setStartingNodeId] = useState<string | null>(null);
  const [startError, setStartError] = useState<string | null>(null);

  const handleStartSession = async (node: StackNode) => {
    if (!client) return;
    setStartingNodeId(node.nodeId);
    setStartError(null);
    try {
      const res = await client.startSession({
        sessionToken,
        projectId: session.projectId,
        toolPath: "",
        agent: "",
        recipe: node.childRecipe,
        stackParent: session.sessionId,
        sessionType: "",
        model: "",
        permissionMode: "",
        initialPrompt: "",
        sandbox: false,
        branchWorktreeIntent: "new_branch_from_base",
        newBranchName: "",
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

  return (
    <div data-testid="pr-stack-screen" className="flex-1 min-h-0 flex overflow-hidden">
      <div className="w-1/2 min-w-0 border-r border-border flex flex-col overflow-hidden">
        {startError && (
          <p className="text-xs text-destructive px-3 pt-2" role="alert">
            {startError}
          </p>
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
        />
      </div>
    </div>
  );
}
