import React, { useCallback, useMemo, useState } from "react";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { SessionService, SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { useHostPresence } from "tddy-client-web";
import { UploadProgressProvider } from "tddy-terminal-web";
import { requestSessionsRefresh } from "tddy-session-web";
import { sessionNotificationRegistry } from "tddy-session-web";
import { useSessionNotifications } from "tddy-session-web";
import { useAuthContext } from "../../hooks/authProvider";
import { hostNotConnected } from "tddy-client-web";
import { TooltipProvider, detectIsMobile, useIsMobile } from "tddy-ui-web";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "tddy-session-web";
import { SessionHostStatsFooter } from "./SessionHostStatsFooter";
import { useSessionAttachment } from "tddy-session-web";
import { SessionShell } from "tddy-session-web";
import { sessionsDrawerPathForSession, SESSIONS_DRAWER_ROUTE, SESSIONS_NEW_ROUTE } from "tddy-client-web";
import { resolveShortcutsForSession } from "tddy-terminal-web";
import { isCliSessionType } from "tddy-session-create-web";
import { registerSessionTypeOptions, sessionTypeOptions } from "tddy-session-create-web";
import { useSelectableAgents } from "tddy-session-create-web";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { useSessionsDrawerInspector } from "./useSessionsDrawerInspector";
import { useSessionsDrawerClients } from "./useSessionsDrawerClients";
import { useProjectHomeSetup } from "./useProjectHomeSetup";
import { ProjectHomeSetupPane } from "tddy-session-create-web";
import { useSessionsDrawerAttachClaims } from "./useSessionsDrawerAttachClaims";
import { useSessionsDrawerProjects } from "./useSessionsDrawerProjects";
import { useSessionsDrawerProjectHomes } from "./useSessionsDrawerProjectHomes";
import { useEnsureProjectHome } from "./useEnsureProjectHome";
import { useSessionsDrawerRuntimeRegistry } from "./useSessionsDrawerRuntimeRegistry";
import { useSessionsDrawerSessionList } from "./useSessionsDrawerSessionList";
import { useSessionsDrawerRuntimeAttachment } from "./useSessionsDrawerRuntimeAttachment";
import { useSessionsDrawerOwningHost } from "./useSessionsDrawerOwningHost";
import { useSessionsDrawerActivation } from "./useSessionsDrawerActivation";
import { useSessionsDrawerDeleteSelection } from "./useSessionsDrawerDeleteSelection";
import { SessionsDrawerOpenListButton, SessionsDrawerUnknownSession } from "./SessionsDrawerScreenParts";
import { useSessionsDrawerPaneActions } from "./useSessionsDrawerPaneActions";
import { useResumedSessionLiveness } from "./useResumedSessionLiveness";
import { sessionPaneClients } from "./sessionPaneClients";
import {
  SessionsDrawerLoadNotices,
  SessionsDrawerPaneError,
  sessionsDrawerListNotices,
} from "./SessionsDrawerNotices";
import { useHostConnector, useHostLabel, useSelectedDaemon } from "tddy-client-web";

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
  const { sessionToken: authSessionToken, user } = useAuthContext();
  const sessionToken = authSessionToken ?? "";
  // The signed-in user's login. An agent-conversation tab needs it to cancel when it is closed; the
  // daemon attributes the cancel itself. `null` when no one is signed in: the cancel is refused.
  const callerLogin = user?.login ? user.login : null;

  const drawerClients = useSessionsDrawerClients();
  const { client, projectClient, catalogClient } = drawerClients;

  // One daemon-level notification feed for the whole drawer, however many rows it has (NFR1). The
  // dots are written into `sessionNotificationRegistry`, which each row reads for itself; what comes
  // back is only whether the feed is still live, shown above the drawer when it is not.
  const { error: notificationsError } = useSessionNotifications();

  const { selectedInstanceId } = useSelectedDaemon();
  // The common room, asked for by name rather than taken off the shared context. Three things in
  // this screen want it and all three are presence: the cross-host session roster below, the
  // session-client stand-in in tests, and the terminal panes' own participant reads. A host reached
  // over a wire with no roster yields `null`, and each of those already handles its absence.
  const room = useHostPresence(selectedInstanceId);
  // Address any daemon's SessionService directly. Used to connect to a cross-host row's owning
  // daemon at click time, when the owner is known but the selected session (and thus `activeClient`)
  // hasn't updated yet — a host named that late cannot have a hook of its own, so it is resolved
  // through the connection registry rather than by `useDaemonClientFor`.
  const connectHost = useHostConnector();
  const clientForHost = useCallback(
    (instanceId: string): Client<typeof SessionService> | null =>
      connectHost(instanceId)?.clientFor(SessionService) ?? null,
    [connectHost],
  );

  const {
    location,
    navigate,
    setParams,
    selectedSessionId,
    mode,
    inspectorTabParam,
    inspectorState,
    applyInspectorState,
    handleInspectorToggle,
    handleInspectorClose,
    handleInspectorExpand,
    handleInspectorRestore,
  } = useSessionsDrawerInspector();

  const claims = useSessionsDrawerAttachClaims();
  const { attachClaimRef, settleAttach, listGenerationRef, awaitingCreatedSessionIdRef, unknownSession, setUnknownSession } =
    claims;

  const { projects, error: projectsError, reload: reloadProjects } = useSessionsDrawerProjects(projectClient, sessionToken);
  // Main is per host: the pinned entries and the setup pane read the selected host's rows, the host
  // the ensure below targets.
  const { homes: projectHomes, projectOnHost } = useSessionsDrawerProjectHomes(projects, selectedInstanceId);
  const projectHomeSetup = useProjectHomeSetup();
  const setup = projectHomeSetup.setup;

  // Default closed on mobile (the open 280px panel would cover the main pane);
  // open on desktop.
  const [sessionListOpen, setSessionListOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const attachmentControls = useSessionAttachment();
  const { state: attachment, hint: attachmentHint, connectSession, restore: restoreAttachment, reset: resetAttachment } =
    attachmentControls;

  const connectedSessionId =
    attachment.status === "connected" ? attachment.connection.sessionId : null;
  const {
    runtimeRegistry,
    runtimes,
    buildSessionExecToolClient,
    buildSessionActivityClient,
    onSessionRegisterInsert,
    onSessionBytes,
    handleInsertPathIntoTerminal,
  } = useSessionsDrawerRuntimeRegistry(connectedSessionId);

  const {
    crossHostSessionsVisible,
    sortedSessions,
    listLoadError,
    addOptimisticSession,
    sessionMetadataBySessionId,
    selectedSession,
    listGeneration,
    drawerSessions,
  } = useSessionsDrawerSessionList({
    client,
    sessionToken,
    room,
    selectedInstanceId,
    selectedSessionId,
    listGenerationRef,
  });

  const { onSessionDisconnect, selectedTraffic } = useSessionsDrawerRuntimeAttachment({
    attachment,
    attachmentHint,
    resetAttachment,
    connectedSessionId,
    runtimeRegistry,
    runtimes,
    sortedSessions,
    selectedSession,
    attachClaimRef,
  });

  const owningHost = useSessionsDrawerOwningHost(selectedSession, selectedInstanceId);
  const { activeClient, activeHost } = owningHost;

  // A daemon instance id as a host is named in text (see `rpc/hostLabel`) — the owning-host badge on
  // cross-host rows, and every "not connected" this screen says.
  const hostLabelForInstance = useHostLabel();

  // A successful resume reads the session as live until the listing says so (the pane only).
  const { liveSelectedSession, markSessionResumed } = useResumedSessionLiveness(sortedSessions, selectedSession);

  const { paneError, reportAttachFailure, handleResume, resume, handleDelete, handleTerminate } =
    useSessionsDrawerPaneActions({
      selectedSession,
      selectedSessionId,
      selectedInstanceId,
      attachment: attachmentControls,
      activeHost,
      activeClient,
      sessionToken,
      attachClaimRef,
      listGeneration,
      onResumed: markSessionResumed,
    });

  // Key-press shortcuts for the connected session's tool (shown as the mobile overlay).
  const mobileShortcuts = useMemo(
    () =>
      resolveShortcutsForSession(
        isCliSessionType(selectedSession?.agent ?? ""),
        selectedSession?.tool ?? "",
      ),
    [selectedSession],
  );
  useSessionsDrawerActivation({
    claims,
    location,
    setParams,
    selectedSessionId,
    inspectorTabParam,
    inspectorState,
    applyInspectorState,
    sortedSessions,
    selectedSession,
    listGeneration,
    runtimeRegistry,
    attachment,
    connectSession,
    restoreAttachment,
    resetAttachment,
    connectHost,
    selectedInstanceId,
    sessionToken,
    hostLabelForInstance,
    reportAttachFailure,
  });

  // Selecting a session is a navigation plus one acknowledgement — the activation effect above does
  // the inspector and attachment work, so Back and a pasted link get the same treatment for
  // everything that decides what the screen shows.
  const handleSelectSession = (sessionId: string) => {
    // The exception is the row's own indicator: clicking a row is the operator *deciding* to look at
    // what that session had to say, so its outstanding notifications settle here (FR6). It sits on
    // the click rather than in the activation effect deliberately — a reload or a Back onto a
    // session the operator is not reading should not silently clear a dot they never saw. Only the
    // notification-driven half clears: a `pending_elicitation` is an unanswered gate and keeps its
    // yellow until it is actually answered (see `sessionIndicatorFor`).
    sessionNotificationRegistry.markSeen(sessionId, Date.now());
    // On mobile the list is a full-screen overlay — close it so the terminal is visible.
    if (isMobile) setSessionListOpen(false);
    navigate(sessionsDrawerPathForSession(sessionId));
  };

  const {
    selectedForDelete,
    selectionMode,
    toggleSelectForDelete,
    enterSelectionMode,
    exitSelectionMode,
    toggleSelectAll,
    handleBulkDelete,
    bulkDeleteFailures,
  } = useSessionsDrawerDeleteSelection({
    sortedSessions,
    selectedInstanceId,
    clientForHost,
    sessionToken,
    hostLabelForInstance,
    onSessionDeleted: (sessionId) => {
      // What the single-row delete does on success: the pane stops holding a session that is gone.
      if (sessionId === selectedSessionId) resetAttachment();
    },
  });

  const handleUnknownHome = () => {
    setUnknownSession(false);
    navigate(SESSIONS_DRAWER_ROUTE);
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

  const handleCreateSession = () => navigate(SESSIONS_NEW_ROUTE);
  const handleCancelCreate = () => navigate(SESSIONS_DRAWER_ROUTE);
  const handleSessionCreated = (sessionId: string) => {
    // Auto-close the sessions drawer so the new session's terminal is unobstructed.
    setSessionListOpen(false);
    awaitingCreatedSessionIdRef.current = sessionId;
    navigate(sessionsDrawerPathForSession(sessionId));
    const creatingHost = connectHost(selectedInstanceId);
    if (creatingHost) {
      // Never rejects — a failed attach is the attachment's `error` state.
      void connectSession(sessionId, sessionToken, creatingHost).then((attached) =>
        settleAttach(sessionId, attached),
      );
    } else {
      // The session was created, but the wire to its host is gone: say so on its pane rather than
      // leaving it blank. The liveness effect attaches once a wire returns.
      reportAttachFailure(hostNotConnected(hostLabelForInstance(selectedInstanceId ?? "")));
    }
    // Refresh the sessions list so the newly-created session appears in the drawer
    // and selectedSession resolves to a non-null value.
    requestSessionsRefresh();
  };

  const openProjectHomeSetup = (projectId: string, replacing = false) => {
    if (isMobile) setSessionListOpen(false);
    projectHomeSetup.open(projectId, { replacing });
  };
  // The pinned main entry ensures main on the selected host and selects it, or sends a project with
  // no main to set one up; a main the setup pane starts is attached like a created session.
  const homeHandlers = {
    onStarted: (sessionId: string) => {
      reloadProjects();
      handleSessionCreated(sessionId);
    },
    onNoMain: (projectId: string) => {
      reloadProjects();
      openProjectHomeSetup(projectId);
    },
  };
  const { ensure: ensureProjectHome, start: startProjectHome } =
    useEnsureProjectHome(client, sessionToken, handleSelectSession, homeHandlers);

  return (
    <UploadProgressProvider>
    <TooltipProvider delayDuration={0}>
      {catalogClient && (
        <SessionTypeOptionsRegistrar
          catalogClient={catalogClient}
          instanceId={selectedInstanceId ?? ""}
        />
      )}
      {/* 100dvh (via AppShell fullbleed): on mobile 100vh includes the area behind the browser
          chrome, which would push the bottom keyboard bar off the visible screen. */}
      <SessionShell
        variant="fullbleed"
        title="Sessions"
        onNavigate={onNavigate}
        dataTestId="sessions-drawer-screen"
        requiresHost
      >
        <SessionsDrawerLoadNotices projectsError={projectsError} notificationsError={notificationsError} />
        <div className="flex flex-1 min-h-0 overflow-hidden relative">
          {isMobile && !sessionListOpen && (
            <SessionsDrawerOpenListButton onOpen={() => setSessionListOpen(true)} />
          )}
          <SessionDrawer
            sessions={drawerSessions}
            selectedSessionId={selectedSessionId}
            onSelectSession={handleSelectSession}
            onCreateSession={handleCreateSession}
            isOpen={sessionListOpen}
            onClose={() => setSessionListOpen(false)}
            onOpen={() => setSessionListOpen(true)}
            isMobile={isMobile}
            selectedInstanceId={selectedInstanceId ?? ""}
            crossHostSessionsVisible={crossHostSessionsVisible}
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
            notices={sessionsDrawerListNotices(listLoadError, bulkDeleteFailures)}
            projects={projects}
            projectHomes={projectHomes}
            onEnsureProjectHome={(projectId) =>
              ensureProjectHome(projectId, selectedInstanceId ?? "")
            }
            onSetUpProjectHome={openProjectHomeSetup}
            onReplaceProjectHome={(projectId) => openProjectHomeSetup(projectId, true)}
          />
          {!unknownSession && <SessionsDrawerPaneError message={paneError} />}
          {/* A bad deep link surfaces "not found" in the detail pane only — the session list
              stays visible so the operator can pick a valid session. */}
          {setup !== null ? (
            <ProjectHomeSetupPane
              {...setup}
              project={projectOnHost(setup.projectId)}
              daemonInstanceId={selectedInstanceId ?? ""}
              sessionToken={sessionToken}
              onStart={(spec) => startProjectHome(setup.projectId, selectedInstanceId ?? "", spec)}
              onClose={projectHomeSetup.close}
            />
          ) : unknownSession ? (
            <SessionsDrawerUnknownSession onHome={handleUnknownHome} />
          ) : (
            <SessionMainPane
              selectedSession={liveSelectedSession}
              attachment={attachment}
              attachmentHint={attachmentHint}
              inspectorState={inspectorState}
              onToggleInspector={handleInspectorToggle}
              onInspectorClose={handleInspectorClose}
              onInspectorExpand={handleInspectorExpand}
              onInspectorRestore={handleInspectorRestore}
              onResume={handleResume}
              onResumeSession={resume}
              onDelete={handleDelete}
              onTerminate={handleTerminate}
              isCreating={mode === "creating"}
              {...sessionPaneClients(mode === "creating", drawerClients, owningHost)}
              host={activeHost}
              sessionToken={sessionToken}
              callerLogin={callerLogin}
              onCancelCreate={handleCancelCreate}
              onSessionCreated={handleSessionCreated}
              room={room}
              mobileShortcuts={mobileShortcuts}
              onChildSessionStarted={handleChildSessionStarted}
              onSwitchPeer={handleSelectSession}
              traffic={selectedTraffic}
              projects={projects}
              runtimes={runtimes}
              sessions={sortedSessions}
              focusedRuntimeId={runtimeRegistry.focusedSessionId}
              onSessionRegisterInsert={onSessionRegisterInsert}
              onInsertPathIntoTerminal={handleInsertPathIntoTerminal}
              onSessionDisconnect={onSessionDisconnect}
              onSessionBytes={onSessionBytes}
              buildSessionExecToolClient={buildSessionExecToolClient}
              buildSessionActivityClient={buildSessionActivityClient}
              sessionMetadataBySessionId={sessionMetadataBySessionId}
            />
          )}
        </div>
        <SessionHostStatsFooter
          attachment={attachment}
          runtimes={runtimes}
          runtimeRegistry={runtimeRegistry}
        />
      </SessionShell>
    </TooltipProvider>
    </UploadProgressProvider>
  );
}

/**
 * Registers the session types the selected daemon serves, from its `ListAgents` answer, for
 * readers that hold no catalog of their own (a terminal tab's label, the "+" menu's kinds).
 * Without a listing of its own this screen left those readers with only `tool` until some
 * Create pane happened to mount. Renders nothing.
 */
function SessionTypeOptionsRegistrar({
  catalogClient,
  instanceId,
}: {
  catalogClient: Client<typeof CatalogService>;
  instanceId: string;
}) {
  const { agents } = useSelectableAgents(catalogClient, instanceId);
  registerSessionTypeOptions(sessionTypeOptions(agents, instanceId, instanceId));
  return null;
}
