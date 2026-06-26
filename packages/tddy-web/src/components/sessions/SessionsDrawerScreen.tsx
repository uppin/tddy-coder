import React, { useEffect, useMemo, useRef, useState } from "react";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { sortSessionsByCreation } from "../../utils/sessionSort";
import { useHttpClient } from "../../rpc/transportProvider";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { SessionTrafficBar } from "./SessionTrafficBar";
import { useSessionAttachment } from "./useSessionAttachment";
import { nextInspectorState } from "./inspectorState";
import { useTerminalControl } from "./useTerminalControl";
import { sessionsDrawerPathForSession, parseSessionsDrawerSessionId } from "../../routing/appRoutes";
import { Signal } from "../../gen/connection_pb";
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
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    return parseSessionsDrawerSessionId(window.location.hash.slice(1));
  });
  const [inspectorState, setInspectorState] = useState<InspectorDrawerState>("closed");
  // Track whether the URL-seeded selectedSessionId has been activated after the session list loads
  const deepLinkActivatedRef = useRef(false);
  const [mode, setMode] = useState<"list" | "creating">("list");

  const [sessionListOpen, setSessionListOpen] = useState<boolean>(() => {
    if (typeof window === "undefined") return true;
    const isMobile = window.innerWidth < 768;
    const hasSelection = parseSessionsDrawerSessionId(window.location.hash.slice(1)) !== null;
    return !(isMobile && hasSelection);
  });

  const { state: attachment, connectSession, resumeSession, deleteSession, signalSession } = useSessionAttachment();

  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

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

  // When the session list loads and a session was pre-selected from the URL hash,
  // activate it (set inspector state + auto-connect) exactly once.
  useEffect(() => {
    if (deepLinkActivatedRef.current) return;
    if (!selectedSessionId || sortedSessions.length === 0) return;
    const session = sortedSessions.find((s) => s.sessionId === selectedSessionId);
    if (!session) return;
    deepLinkActivatedRef.current = true;
    setInspectorState(
      nextInspectorState(
        { open: false, expanded: false },
        { type: "select", isActive: session.isActive },
      ).open
        ? "open"
        : "closed",
    );
    if (session.isActive) {
      connectSession(selectedSessionId, sessionToken, client).catch((err) => {
        console.debug("[SessionsDrawerScreen] deep-link connectSession error", err);
      });
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sortedSessions]);

  // React to attachment status changes — open inspector for non-connected states, close when connected.
  useEffect(() => {
    if (!selectedSessionId) return;
    const isConnected =
      attachment.status === "connected-livekit" ||
      attachment.status === "connected-grpc";
    if (isConnected) {
      setInspectorState("closed");
    } else if (attachment.status === "idle" || attachment.status === "error") {
      setInspectorState((prev) => (prev === "expanded" ? "expanded" : "open"));
    }
    // "connecting": no change — preserve current state during the handshake
  }, [attachment.status, selectedSessionId]);

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
    // Auto-close the session list on mobile when a session is selected
    if (typeof window !== "undefined" && window.innerWidth < 768) {
      setSessionListOpen(false);
    }
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
    signalSession(sessionId, Signal.SIGTERM, sessionToken, client).catch((err) => {
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
    // Refresh the sessions list so the newly-created session appears in the drawer
    // and selectedSession resolves to a non-null value.
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        setSessions(resp.sessions as SessionEntry[]);
      })
      .catch((err) => {
        console.debug("[SessionsDrawerScreen] listSessions after create error", err);
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
          isOpen={sessionListOpen}
          onClose={() => setSessionListOpen(false)}
          onOpen={() => setSessionListOpen(true)}
        />
        <div className="flex-1 min-w-0 flex flex-col h-full overflow-hidden">
          {selectedSession && isConnected && <SessionTrafficBar attachment={attachment} />}
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
      </div>
    </TooltipProvider>
  );
}
