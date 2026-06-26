import React, { useEffect, useMemo, useState } from "react";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { sortSessionsByCreation } from "../../utils/sessionSort";
import { useHttpClient } from "../../rpc/transportProvider";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { useSessionAttachment } from "./useSessionAttachment";
import { nextInspectorState } from "./inspectorState";
import { useTerminalControl } from "./useTerminalControl";
import { sessionsDrawerPathForSession } from "../../routing/appRoutes";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export function SessionsDrawerScreen() {
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";

  const client = useHttpClient(ConnectionService);

  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [inspectorState, setInspectorState] = useState<InspectorDrawerState>("closed");
  const [mode, setMode] = useState<"list" | "creating">("list");

  const { state: attachment, connectSession, resumeSession, deleteSession, signalSession } = useSessionAttachment();

  const connectedSessionId =
    attachment.status === "connected-grpc" || attachment.status === "connected-livekit"
      ? attachment.sessionId
      : null;
  const { controlState, claim: claimControl } = useTerminalControl(connectedSessionId, sessionToken);

  // Fetch sessions on mount
  useEffect(() => {
    let cancelled = false;
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (!cancelled) {
          setSessions(resp.sessions as SessionEntry[]);
        }
      })
      .catch((err) => {
        console.debug("[SessionsDrawerScreen] listSessions error", err);
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  const sortedSessions = useMemo(() => sortSessionsByCreation(sessions), [sessions]);

  const selectedSession = useMemo(
    () => sortedSessions.find((s) => s.sessionId === selectedSessionId) ?? null,
    [sortedSessions, selectedSessionId],
  );

  // When a session is selected in the drawer, auto-connect if it is active
  const handleSelectSession = (sessionId: string) => {
    setSelectedSessionId(sessionId);
    const session = sortedSessions.find((s) => s.sessionId === sessionId);
    // Update inspector state based on session active status
    setInspectorState((prev) =>
      nextInspectorState(
        { open: prev !== "closed", expanded: prev === "expanded" },
        { type: "select", isActive: session?.isActive ?? false },
      ).open
        ? "open"
        : "closed",
    );
    if (session?.isActive) {
      connectSession(sessionId, sessionToken, client).catch((err) => {
        console.debug("[SessionsDrawerScreen] connectSession error", err);
      });
    }
  };

  const handleResume = (sessionId: string) => {
    resumeSession(sessionId, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] resumeSession error", err);
    });
  };

  const handleDelete = (sessionId: string) => {
    deleteSession(sessionId, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] deleteSession error", err);
    });
  };

  const handleTerminate = (sessionId: string) => {
    // SIGTERM = 15
    signalSession(sessionId, 15, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] signalSession error", err);
    });
  };

  const handleInspectorToggle = () => {
    setInspectorState((prev) => {
      const prevState = { open: prev !== "closed", expanded: prev === "expanded" };
      const next = nextInspectorState(prevState, { type: "toggle" });
      return next.open ? (next.expanded ? "expanded" : "open") : "closed";
    });
  };

  const handleInspectorClose = () => setInspectorState("closed");
  const handleInspectorExpand = () => setInspectorState("expanded");
  const handleInspectorRestore = () => setInspectorState("open");

  const handleCreateSession = () => setMode("creating");
  const handleCancelCreate = () => setMode("list");
  const handleSessionCreated = (sessionId: string) => {
    setMode("list");
    setSelectedSessionId(sessionId);
    window.location.hash = sessionsDrawerPathForSession(sessionId);
    connectSession(sessionId, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] connectSession after create error", err);
    });
  };

  return (
    <TooltipProvider delayDuration={0}>
      <div
        data-testid="sessions-drawer-screen"
        className="flex h-screen w-full overflow-hidden font-sans text-foreground"
      >
        <SessionDrawer
          sessions={sortedSessions}
          selectedSessionId={selectedSessionId}
          onSelectSession={handleSelectSession}
          onCreateSession={handleCreateSession}
        />
        <SessionMainPane
          selectedSession={selectedSession}
          attachment={attachment}
          inspectorState={inspectorState}
          onToggleInspector={handleInspectorToggle}
          onInspectorClose={handleInspectorClose}
          onInspectorExpand={handleInspectorExpand}
          onInspectorRestore={handleInspectorRestore}
          onResume={handleResume}
          onDelete={handleDelete}
          onTerminate={handleTerminate}
          isCreating={mode === "creating"}
          client={client}
          sessionToken={sessionToken}
          onCancelCreate={handleCancelCreate}
          onSessionCreated={handleSessionCreated}
          terminalControl={connectedSessionId ? { ...controlState, onClaim: claimControl } : undefined}
        />
      </div>
    </TooltipProvider>
  );
}
