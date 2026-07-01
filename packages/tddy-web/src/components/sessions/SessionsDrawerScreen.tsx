import React, { useEffect, useMemo, useRef, useState } from "react";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { TokenService } from "../../gen/token_pb";
import { sortSessionsByCreation } from "../../utils/sessionSort";
import { useHttpClient } from "../../rpc/transportProvider";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { StatusBar } from "./StatusBar";
import { useSessionAttachment } from "./useSessionAttachment";
import { nextInspectorState } from "./inspectorState";
import { useTerminalControl } from "./useTerminalControl";
import { sessionsDrawerPathForSession, parseSessionsDrawerSessionId } from "../../routing/appRoutes";
import { Signal } from "../../gen/connection_pb";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { detectIsMobile, useIsMobile } from "../../hooks/useIsMobile";
import { resolveShortcutsForSession } from "../../lib/toolShortcuts";
import { isClaudeCliSession } from "../../constants/claudeCliModels";
import { PanelLeftOpen } from "lucide-react";
// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export function SessionsDrawerScreen() {
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";

  const client = useHttpClient(ConnectionService);
  const tokenClient = useHttpClient(TokenService);

  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(() => {
    if (typeof window === "undefined") return null;
    return parseSessionsDrawerSessionId(window.location.hash.slice(1));
  });
  const [inspectorState, setInspectorState] = useState<InspectorDrawerState>("closed");
  // Track whether the inspector was auto-opened (by session select) vs manually opened by the user.
  // When true, a successful connection should auto-close the inspector.
  const inspectorAutoOpenRef = useRef(false);
  // Track whether the URL-seeded selectedSessionId has been activated after the session list loads
  const deepLinkActivatedRef = useRef(false);
  const [mode, setMode] = useState<"list" | "creating">("list");

  // Default closed on mobile (the open 280px panel would cover the main pane);
  // open on desktop.
  const [sessionListOpen, setSessionListOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const { state: attachment, connectSession, resumeSession, deleteSession, signalSession, reset: resetAttachment } = useSessionAttachment();

  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  const connectedSessionId =
    attachment.status === "connected-grpc" || attachment.status === "connected-livekit"
      ? attachment.sessionId
      : null;
  const { controlState, controlTokenRef, claim: claimControl } = useTerminalControl(connectedSessionId, sessionToken);

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

  // Key-press shortcuts for the connected session's tool (shown as the mobile overlay).
  const mobileShortcuts = useMemo(
    () =>
      resolveShortcutsForSession(
        isClaudeCliSession(selectedSession?.agent ?? ""),
        selectedSession?.tool ?? "",
      ),
    [selectedSession],
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

  // React to attachment status changes:
  // - "connected-*" for the selected session → close inspector IF it was auto-opened
  // - "idle" for an INACTIVE selected session → auto-open inspector
  // - "error" → open inspector so the user sees the problem
  // - "connecting" → no change (preserve state during handshake)
  useEffect(() => {
    if (!selectedSessionId) return;
    if (
      attachment.status === "connected-livekit" ||
      attachment.status === "connected-grpc"
    ) {
      if (
        attachment.sessionId === selectedSessionId &&
        inspectorAutoOpenRef.current
      ) {
        inspectorAutoOpenRef.current = false;
        setInspectorState("closed");
      }
    } else if (attachment.status === "idle") {
      // Only auto-open for inactive sessions; active sessions will connect shortly
      const session = sortedSessions.find((s) => s.sessionId === selectedSessionId);
      if (session && !session.isActive) {
        setInspectorState((prev) => (prev === "expanded" ? "expanded" : "open"));
      }
    } else if (attachment.status === "error") {
      setInspectorState((prev) => (prev === "expanded" ? "expanded" : "open"));
    }
  }, [attachment.status, selectedSessionId, sortedSessions]);

  // When a session is selected in the drawer, auto-connect if it is active
  const handleSelectSession = (sessionId: string) => {
    // Reset attachment so useEffect re-evaluates state for the newly selected session
    resetAttachment();
    setSelectedSessionId(sessionId);
    // On mobile the list is a full-screen overlay — close it so the terminal is visible.
    if (isMobile) setSessionListOpen(false);
    const session = sortedSessions.find((s) => s.sessionId === sessionId);
    const willOpen = nextInspectorState(
      { open: false, expanded: false },
      { type: "select", isActive: session?.isActive ?? false },
    ).open;
    // Track auto-open so we know whether a subsequent connect should auto-close
    inspectorAutoOpenRef.current = willOpen;
    setInspectorState(willOpen ? "open" : "closed");
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

  const refreshSessions = () => {
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        setSessions(resp.sessions as SessionEntry[]);
      })
      .catch((err) => {
        console.debug("[SessionsDrawerScreen] listSessions refresh error", err);
      });
  };

  const handleTerminate = (sessionId: string) => {
    signalSession(sessionId, Signal.SIGTERM, sessionToken, client)
      .catch((err) => {
        // Common cause: the session already ended (e.g. process exited on its own) before this
        // click reached the daemon — refreshSessions() below still corrects the stale `isActive`
        // that caused the "Terminate" button to be shown for an already-dead session.
        console.debug("[SessionsDrawerScreen] signalSession error", err);
      })
      .finally(() => {
        // The daemon computes `isActive` from live PID liveness, not from a push update — refetch
        // so the row (and its "Terminate" button) reflects the session's actual current state.
        refreshSessions();
      });
  };

  const handleInspectorToggle = () => {
    // Manual interaction — subsequent connection should not auto-close the inspector
    inspectorAutoOpenRef.current = false;
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
        // 100dvh (not 100vh/h-screen): on mobile 100vh includes the area behind the
        // browser chrome, which pushes the bottom keyboard bar off the visible screen.
        className="flex flex-col h-[100dvh] w-full overflow-hidden font-sans text-foreground"
      >
        <StatusBar attachment={attachment} />
        <div className="flex flex-1 min-h-0 overflow-hidden relative">
          {isMobile && !sessionListOpen && (
            <button
              type="button"
              data-testid="sessions-drawer-open-overlay-btn"
              onClick={() => setSessionListOpen(true)}
              title="Open session list"
              className="absolute top-2 left-2 z-20 flex items-center justify-center h-9 w-9 rounded-md border border-border bg-background/90 text-foreground shadow-md backdrop-blur-sm hover:bg-muted transition-colors"
            >
              <PanelLeftOpen className="h-5 w-5" />
            </button>
          )}
          <SessionDrawer
            sessions={sortedSessions}
            selectedSessionId={selectedSessionId}
            onSelectSession={handleSelectSession}
            onCreateSession={handleCreateSession}
            isOpen={sessionListOpen}
            onClose={() => setSessionListOpen(false)}
            onOpen={() => setSessionListOpen(true)}
            isMobile={isMobile}
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
            tokenClient={tokenClient}
            sessionToken={sessionToken}
            onCancelCreate={handleCancelCreate}
            onSessionCreated={handleSessionCreated}
            terminalControl={connectedSessionId ? { ...controlState, onClaim: claimControl } : undefined}
            controlTokenRef={connectedSessionId ? controlTokenRef : undefined}
            onDisconnect={resetAttachment}
            mobileShortcuts={mobileShortcuts}
          />
        </div>
      </div>
    </TooltipProvider>
  );
}
