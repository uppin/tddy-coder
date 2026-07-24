import React, { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { create } from "@bufbuild/protobuf";
import { createClient, type Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import {
  ConnectionService,
  SessionEntrySchema,
  type SessionEntry,
  type ProjectEntry,
} from "../../gen/connection_pb";
import { TokenService } from "../../gen/token_pb";
import {
  useHttpClient,
  useLiveKitTransportFactory,
  useLiveKitTransportFactoryIsOverridden,
} from "../../rpc/transportProvider";
import { useDaemonClient, useDaemonClientFor, useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { owningHostForSession } from "../../utils/crossHostSessions";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { useSessionManager } from "./sessionManager";
import { SessionRuntimeRegistry, type SessionRuntimeConnection } from "./sessionRuntimeRegistry";
import { useAuthContext } from "../../hooks/authProvider";
import { AppShell } from "../shell/AppShell";
import { Button } from "../ui/button";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { HostStatsFooter } from "./HostStatsFooter";
import { useSessionAttachment, type SessionAttachmentState } from "./useSessionAttachment";
import { nextInspectorState } from "./inspectorState";
import { sessionsDrawerPathForSession, parseSessionsDrawerSessionId } from "../../routing/appRoutes";
import { Signal } from "../../gen/connection_pb";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { detectIsMobile, useIsMobile } from "../../hooks/useIsMobile";
import { resolveShortcutsForSession } from "../../lib/toolShortcuts";
import { isCliTerminalSession } from "../../constants/claudeCliModels";
import { PanelLeftOpen } from "lucide-react";
// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

export function SessionsDrawerScreen({
  // Optional so isolated component tests can mount the screen without a router; production
  // (index.tsx) always wires the hash-router navigate.
  onNavigate = () => {},
}: {
  onNavigate?: (path: string) => void;
}) {
  const { sessionToken: authSessionToken } = useAuthContext();
  const sessionToken = authSessionToken ?? "";

  // ConnectionService is daemon-level RPC — routed over the shared common-room LiveKit
  // connection to whichever daemon is currently selected (see `SelectedDaemonProvider`).
  // `null` until a daemon is selected / the room is connected; every call site below guards.
  // The selected-daemon `client` still owns the CREATE flow (a new session is created on the
  // selected host); cross-host interaction routes through `activeClient` (computed below).
  const client = useDaemonClient(ConnectionService);
  const { room, selectedInstanceId } = useSelectedDaemon();
  const daemons = useDaemons();
  const liveKitFactory = useLiveKitTransportFactory();
  // TokenService issues this session's own browser LiveKit-join token — it must stay HTTP to the
  // serving daemon (you cannot fetch a LiveKit-join token *over* LiveKit), per the PRD's bootstrap
  // exception. Do not migrate this to useDaemonClient.
  const tokenClient = useHttpClient(TokenService);

  // Address any daemon's ConnectionService directly (`daemon-{instanceId}`) over the shared
  // common-room connection. Used to connect to a cross-host row's owning daemon at click time, when
  // the owner is known but the selected session (and thus `activeClient`) hasn't updated yet.
  const clientForHost = useCallback(
    (instanceId: string): Client<typeof ConnectionService> | null =>
      room && instanceId
        ? createClient(ConnectionService, liveKitFactory(room, daemonRpcIdentity(instanceId)))
        : null,
    [room, liveKitFactory],
  );

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
  // A deep link (`#/sessions/:id`) that resolves to no known session after the list loads shows a
  // not-found state instead of silently no-opping. Driven by local state so it dismisses on Home
  // without depending on a hash change.
  const [unknownSession, setUnknownSession] = useState(false);
  // Sessions ticked for bulk delete — a Set preserves insertion (selection) order, which the bulk
  // delete replays so sessions are removed in the order they were selected.
  const [selectedForDelete, setSelectedForDelete] = useState<Set<string>>(() => new Set());
  // Bulk-selection mode: off by default so the drawer reads as a plain list. The bottom minibar
  // toggles it on, which is what reveals the per-row checkboxes and the Select-all / Delete actions.
  const [selectionMode, setSelectionMode] = useState(false);

  const toggleSelectForDelete = useCallback((sessionId: string) => {
    setSelectedForDelete((prev) => {
      const next = new Set(prev);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  }, []);

  const enterSelectionMode = useCallback(() => setSelectionMode(true), []);

  // Leaving selection mode always clears the tick set — a stale selection must not survive into the
  // next time the operator opens the bar.
  const exitSelectionMode = useCallback(() => {
    setSelectionMode(false);
    setSelectedForDelete(new Set());
  }, []);

  // The project registry (daemon-level RPC over the selected-daemon common-room connection). Used
  // to resolve an unscoped session's project from its `repoPath` before the worktree RPCs — those
  // require a non-empty `project_id`. Falls back to an empty list when no daemon is selected yet or
  // the call fails, so the drawer still renders.
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  useEffect(() => {
    if (!client) {
      setProjects([]);
      return;
    }
    client
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => setProjects([]));
  }, [client, sessionToken]);

  // Default closed on mobile (the open 280px panel would cover the main pane);
  // open on desktop.
  const [sessionListOpen, setSessionListOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const { state: attachment, connectSession, resumeSession, deleteSession, signalSession, restore: restoreAttachment, reset: resetAttachment } = useSessionAttachment();

  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  const connectedSessionId =
    attachment.status === "connected-grpc" || attachment.status === "connected-livekit"
      ? attachment.sessionId
      : null;

  // Per-session runtime registry: one entry per attached session, surviving focus switches.
  // The inspector reads byte counters + last-received from it for active sessions; inactive sessions
  // fall back to the daemon-sourced `SessionEntry` fields (req 5 dual source). Created once and kept
  // in a ref so the `SessionRuntimeRegistry` instance (and its cached `runtimes` snapshot) is stable
  // across renders — must be instantiated before any callback that touches it (buildSessionClient,
  // onSessionRoom, onSessionDisconnect).
  const runtimeRegistryRef = useRef<SessionRuntimeRegistry | null>(null);
  runtimeRegistryRef.current ??= new SessionRuntimeRegistry();
  const runtimeRegistry = runtimeRegistryRef.current;
  const runtimes = useSyncExternalStore(
    (listener) => runtimeRegistry.subscribe(listener),
    () => runtimeRegistry.runtimes,
    () => runtimeRegistry.runtimes,
  );

  // Session-scoped ConnectionService client (targets the coder participant
  // `daemon-{ownerInstanceId}-{sessionId}` = `attachment.livekitServerIdentity`). Built LAZILY — only
  // when the user actually invokes a session-scoped RPC (ExecuteTool, ClaimTerminalControl) — so that
  // lifecycle RPCs (Delete/Signal/Resume/Connect) and the auto-claim-on-attach stay daemon-direct and
  // do not record the session-participant identity. In production the session's own LiveKit `Room`
  // (captured via the terminal's `onRoom`) is the transport room; in tests the test-double
  // `liveKitFactory` ignores its `room` argument, so the common room is an acceptable stand-in.
  const liveKitFactoryIsOverridden = useLiveKitTransportFactoryIsOverridden();
  const buildSessionClient = useCallback((): Client<typeof ConnectionService> | null => {
    if (!connectedSessionId) return null;
    if (attachment.status !== "connected-livekit") return null;
    const targetIdentity = attachment.livekitServerIdentity;
    if (!targetIdentity) return null;
    const sessionRoom =
      runtimeRegistry.get(connectedSessionId)?.room ?? (liveKitFactoryIsOverridden ? room : null);
    if (!sessionRoom) return null;
    return createClient(ConnectionService, liveKitFactory(sessionRoom, targetIdentity));
  }, [connectedSessionId, attachment, room, liveKitFactory, liveKitFactoryIsOverridden, runtimeRegistry]);

  // Capture a session's connected LiveKit `Room` (fired by the terminal after `room.connect`) so
  // `buildSessionClient` can route session-scoped RPCs over the session's own room in production.
  const onSessionRoom = useCallback(
    (sessionId: string, sessionRoom: Room) => {
      runtimeRegistry.setRoom(sessionId, sessionRoom);
    },
    [runtimeRegistry],
  );

  // Disconnect a runtime terminal. Evicts the session's runtime; if it is the focused/attached
  // session, also resets the attachment so the screen re-evaluates state for the next selection.
  const onSessionDisconnect = useCallback(
    (sessionId: string) => {
      runtimeRegistry.disconnect(sessionId);
      if (sessionId === connectedSessionId) resetAttachment();
    },
    [runtimeRegistry, connectedSessionId, resetAttachment],
  );

  // The merged session list, refresh, and change events all live in one place: `SessionManager`. It
  // unions the selected host's sessions (from ListSessions, refreshed via the window-bound
  // `sessionsRefreshBridge`) with the live cross-host sessions observed as common-room coder
  // participants — LiveKit presence is the keep-alive that makes a non-selected host's session visible.
  const participants = useRoomParticipants(room);
  const { sessions: sortedSessions, addOptimisticSession, sessionMetadataBySessionId } = useSessionManager(
    client,
    sessionToken,
    participants,
    selectedInstanceId ?? "",
  );

  const selectedSession = useMemo(
    () => sortedSessions.find((s) => s.sessionId === selectedSessionId) ?? null,
    [sortedSessions, selectedSessionId],
  );

  // Register a session in the runtime registry on successful attach, storing the connection
  // params so the runtime layer can render its terminal independently of the focused attachment.
  // `lastDataReceivedAt` starts at the attach moment so the inspector reads "0s ago" before the
  // first DataReceived event lands. Re-attach (an existing backgrounded runtime) refreshes the
  // connection params without resetting byte counters.
  useEffect(() => {
    if (attachment.status !== "connected-livekit" && attachment.status !== "connected-grpc") return;
    if (!attachment.sessionId) return;
    const conn: SessionRuntimeConnection =
      attachment.status === "connected-livekit"
        ? {
            status: "connected-livekit",
            livekitUrl: attachment.livekitUrl,
            livekitRoom: attachment.livekitRoom,
            livekitServerIdentity: attachment.livekitServerIdentity,
            identity: attachment.identity,
          }
        : { status: "connected-grpc", livekitUrl: "", livekitRoom: "", livekitServerIdentity: "", identity: "" };
    const existing = runtimeRegistry.get(attachment.sessionId);
    if (!existing) {
      runtimeRegistry.add(attachment.sessionId, {
        sessionId: attachment.sessionId,
        attached: true,
        ...conn,
        bytesIn: 0,
        bytesOut: 0,
        lastDataReceivedAt: Date.now(),
      });
    } else {
      runtimeRegistry.updateConnection(attachment.sessionId, conn);
    }
    runtimeRegistry.focus(attachment.sessionId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachment.status, attachment.sessionId]);

  // Compute the inspector traffic source for the selected session: live runtime (active) wins;
  // otherwise the daemon-sourced `SessionEntry` fields (inactive / non-LiveKit sessions).
  const selectedTraffic = useMemo(() => {
    if (!selectedSession) return null;
    const live = runtimeRegistry.get(selectedSession.sessionId);
    if (live) {
      return { bytesIn: live.bytesIn, bytesOut: live.bytesOut, lastDataReceivedAt: live.lastDataReceivedAt };
    }
    const fromEntry = Number(selectedSession.bytesIn ?? 0n) || 0;
    const fromEntryOut = Number(selectedSession.bytesOut ?? 0n) || 0;
    const lastStr = selectedSession.lastDataReceivedAt ?? "";
    const lastNum = lastStr ? Number(lastStr) : null;
    return {
      bytesIn: fromEntry,
      bytesOut: fromEntryOut,
      lastDataReceivedAt: Number.isFinite(lastNum) ? (lastNum as number) : null,
    };
  }, [selectedSession, runtimes, runtimeRegistry]);

  // The daemon that owns the selected session — cross-host interaction (connect, resume, delete,
  // terminate) must reach that daemon, not the selected one.
  const selectedOwningHost = useMemo(
    () => (selectedSession ? owningHostForSession(selectedSession, selectedInstanceId ?? "") : null),
    [selectedSession, selectedInstanceId],
  );
  const activeClient = useDaemonClientFor(ConnectionService, selectedOwningHost);

  // Human-readable host label for a daemon instance id, with the local daemon's " (this daemon)"
  // suffix stripped — used for the owning-host badge on cross-host rows.
  const hostLabelForInstance = useCallback(
    (instanceId: string): string => {
      const host = daemons.find((d) => d.instanceId === instanceId);
      return (host?.label ?? instanceId).replace(/ \(this daemon\)$/, "");
    },
    [daemons],
  );

  // Key-press shortcuts for the connected session's tool (shown as the mobile overlay).
  const mobileShortcuts = useMemo(
    () =>
      resolveShortcutsForSession(
        isCliTerminalSession(selectedSession?.agent ?? ""),
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
    if (!session) {
      // The deep-linked id is not in the loaded list — surface a not-found state once.
      deepLinkActivatedRef.current = true;
      setUnknownSession(true);
      return;
    }
    deepLinkActivatedRef.current = true;
    setInspectorState(
      nextInspectorState(
        { open: false, expanded: false },
        { type: "select", isActive: session.isActive },
      ).open
        ? "open"
        : "closed",
    );
    if (session.isActive && activeClient) {
      connectSession(selectedSessionId, sessionToken, activeClient).catch((err) => {
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

    // Fast path — the session's runtime is already mounted in the registry (it was attached
    // earlier and stays alive across focus switches). Restore the attachment from the registry's
    // stored connection params so the screen re-evaluates state for the newly selected session
    // WITHOUT an RPC round-trip: no re-connect, no fresh ClaimTerminalControl, no token race, and
    // the existing terminal stream keeps flowing. The registry effect below re-focuses it.
    const existing = runtimeRegistry.get(sessionId);
    if (existing?.status === "connected-livekit") {
      restoreAttachment({
        status: "connected-livekit",
        sessionId,
        livekitUrl: existing.livekitUrl ?? "",
        livekitRoom: existing.livekitRoom ?? "",
        livekitServerIdentity: existing.livekitServerIdentity ?? "",
        identity: existing.identity ?? "",
      } satisfies SessionAttachmentState);
      return;
    }
    if (existing?.status === "connected-grpc") {
      restoreAttachment({ status: "connected-grpc", sessionId } satisfies SessionAttachmentState);
      return;
    }

    // Slow path — not yet attached. Reset so the attachment effect re-evaluates state for the
    // new selection, then connect to the clicked session's owning daemon. `activeClient` still
    // reflects the previously selected session at this point (the selection state update is not
    // yet applied), so build the client for this session's owner directly rather than reading
    // `activeClient` here.
    resetAttachment();
    if (session?.isActive) {
      const owner = owningHostForSession(session, selectedInstanceId ?? "");
      const owningClient = clientForHost(owner);
      if (owningClient) {
        connectSession(sessionId, sessionToken, owningClient).catch((err) => {
          console.debug("[SessionsDrawerScreen] connectSession error", err);
        });
      }
    }
  };

  const handleResume = (sessionId: string) => {
    if (!activeClient) return;
    resumeSession(sessionId, sessionToken, activeClient).catch((err) => {
      console.debug("[SessionsDrawerScreen] resumeSession error", err);
    });
  };

  const handleDelete = (sessionId: string) => {
    if (!activeClient) return;
    deleteSession(sessionId, sessionToken, activeClient).catch((err) => {
      console.debug("[SessionsDrawerScreen] deleteSession error", err);
    });
  };

  // Delete every selected session in selection (insertion) order, routing each delete to the
  // session's owning daemon. Sequential so the daemon processes them in the same order the operator
  // ticked the rows, and so a single failure doesn't abandon the remaining deletes silently.
  // Select-all toggles against the full visible list: if every session is already ticked, clear;
  // otherwise tick them all (fresh Set so insertion order matches the current list order).
  const toggleSelectAll = useCallback(() => {
    setSelectedForDelete((prev) => {
      const allIds = sortedSessions.map((s) => s.sessionId);
      const allSelected = allIds.length > 0 && allIds.every((id) => prev.has(id));
      return allSelected ? new Set() : new Set(allIds);
    });
  }, [sortedSessions]);

  const handleBulkDelete = useCallback(async () => {
    const ids = [...selectedForDelete];
    for (const id of ids) {
      const session = sortedSessions.find((s) => s.sessionId === id);
      const owner = session
        ? owningHostForSession(session, selectedInstanceId ?? "")
        : (selectedInstanceId ?? "");
      const targetClient = clientForHost(owner) ?? client;
      if (!targetClient) continue;
      try {
        await deleteSession(id, sessionToken, targetClient);
      } catch (err) {
        console.debug("[SessionsDrawerScreen] bulk deleteSession error", err);
      }
    }
    setSelectedForDelete(new Set());
    setSelectionMode(false);
    requestSessionsRefresh();
  }, [selectedForDelete, sortedSessions, selectedInstanceId, clientForHost, client, deleteSession, sessionToken]);

  const handleUnknownHome = () => {
    setUnknownSession(false);
    setSelectedSessionId(null);
    onNavigate("/sessions");
  };

  // A pr-stack orchestrator's "Start session" CTA spawns a child immediately in the
  // background (no navigation, no auto-connect — the operator stays on the orchestrator's
  // chat screen). Add a minimal optimistic entry so the drawer reflects it right away. Unlike
  // handleTerminate's refresh() (which needs an authoritative isActive from the daemon), a full
  // refetch here isn't safe to chain: the daemon's session-list enrichment may not have indexed the
  // brand-new session yet. The optimistic overlay is merged into the list (a fetched entry with the
  // same id always wins), and its remaining fields fill in on the next fan-out refresh.
  const handleChildSessionStarted = (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => {
    addOptimisticSession(
      create(SessionEntrySchema, {
        sessionId: entry.sessionId,
        recipe: entry.recipe,
        orchestratorSessionId: entry.orchestratorSessionId,
        projectId: entry.projectId,
        isActive: true,
        createdAt: new Date().toISOString(),
      }),
    );
  };

  const handleTerminate = (sessionId: string) => {
    if (!activeClient) return;
    signalSession(sessionId, Signal.SIGTERM, sessionToken, activeClient)
      .catch((err) => {
        // Common cause: the session already ended (e.g. process exited on its own) before this
        // click reached the daemon — refresh() below still corrects the stale `isActive` that
        // caused the "Terminate" button to be shown for an already-dead session.
        console.debug("[SessionsDrawerScreen] signalSession error", err);
      })
      .finally(() => {
        // The daemon computes `isActive` from live PID liveness, not from a push update — refetch
        // so the row (and its "Terminate" button) reflects the session's actual current state.
        requestSessionsRefresh();
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
    // Auto-close the sessions drawer so the new session's terminal is unobstructed.
    setSessionListOpen(false);
    window.location.hash = sessionsDrawerPathForSession(sessionId);
    if (!client) return;
    connectSession(sessionId, sessionToken, client).catch((err) => {
      console.debug("[SessionsDrawerScreen] connectSession after create error", err);
    });
    // Refresh the sessions list so the newly-created session appears in the drawer
    // and selectedSession resolves to a non-null value.
    requestSessionsRefresh();
  };

  return (
    <TooltipProvider delayDuration={0}>
      {/* 100dvh (via AppShell fullbleed): on mobile 100vh includes the area behind the browser
          chrome, which would push the bottom keyboard bar off the visible screen. */}
      <AppShell
        variant="fullbleed"
        title="Sessions"
        onNavigate={onNavigate}
        dataTestId="sessions-drawer-screen"
      >
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
            selectedInstanceId={selectedInstanceId ?? ""}
            hostLabelForInstance={hostLabelForInstance}
            sessionMetadataBySessionId={sessionMetadataBySessionId}
            selectedForDelete={selectedForDelete}
            selectionMode={selectionMode}
            onToggleSelect={selectionMode ? toggleSelectForDelete : undefined}
            onEnterSelectionMode={enterSelectionMode}
            onExitSelectionMode={exitSelectionMode}
            onSelectAll={toggleSelectAll}
            onBulkDelete={() => {
              void handleBulkDelete();
            }}
          />
          {/* A bad deep link surfaces "not found" in the detail pane only — the session list
              stays visible so the operator can pick a valid session. */}
          {unknownSession ? (
            <div className="flex flex-1 min-h-0 items-center justify-center p-6">
              <div
                data-testid="terminal-route-unknown-session"
                className="rounded-md border border-destructive/40 bg-destructive/5 p-4"
              >
                <p className="mb-3 text-sm text-foreground">
                  Session not found or no longer available.
                </p>
                <Button
                  type="button"
                  variant="secondary"
                  data-testid="terminal-route-unknown-session-home"
                  onClick={handleUnknownHome}
                >
                  Back to sessions
                </Button>
              </div>
            </div>
          ) : (
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
              client={mode === "creating" ? (client ?? undefined) : (activeClient ?? client ?? undefined)}
              tokenClient={tokenClient}
              sessionToken={sessionToken}
              onCancelCreate={handleCancelCreate}
              onSessionCreated={handleSessionCreated}
              room={room}
              mobileShortcuts={mobileShortcuts}
              onChildSessionStarted={handleChildSessionStarted}
              traffic={selectedTraffic}
              projects={projects}
              runtimes={runtimes}
              sessions={sortedSessions}
              focusedRuntimeId={runtimeRegistry.focusedSessionId}
              onSessionRoom={onSessionRoom}
              onSessionDisconnect={onSessionDisconnect}
              buildSessionClient={buildSessionClient}
              liveKitFactory={liveKitFactory}
              liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
            />
          )}
        </div>
        <HostStatsFooter attachment={attachment} />
      </AppShell>
    </TooltipProvider>
  );
}
