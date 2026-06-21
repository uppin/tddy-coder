import React, { useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { sortSessionsByCreation } from "../../utils/sessionSort";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionDetailPane } from "./SessionDetailPane";
import { useSessionAttachment } from "./useSessionAttachment";

// ---------------------------------------------------------------------------
// RPC client
// ---------------------------------------------------------------------------

function createConnectionClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(ConnectionService, transport);
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export function SessionsDrawerScreen() {
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";

  const client = useMemo(() => createConnectionClient(), []);

  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);

  const { state: attachment, connectSession, resumeSession } = useSessionAttachment();

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

  const handleDelete = (_sessionId: string) => {
    // TODO: implement delete
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
        />
        <SessionDetailPane
          selectedSession={selectedSession}
          attachment={attachment}
          onConnect={handleSelectSession}
          onResume={handleResume}
          onDelete={handleDelete}
        />
      </div>
    </TooltipProvider>
  );
}
