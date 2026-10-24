import React, { useCallback, useMemo, useState } from "react";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { SessionService, SessionEntrySchema } from "../../gen/session_pb";
import { useHostConnector } from "../../rpc/connections/registry";
import { useHostLabel, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useHostPresence } from "../../rpc/hostDirectory/useHostPresence";
import { UploadProgressProvider } from "../../rpc/uploadProgress";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { sessionNotificationRegistry } from "./sessionNotificationRegistry";
import { useSessionNotifications } from "../../rpc/useSessionNotifications";
import { useAuthContext } from "../../hooks/authProvider";
import { hostNotConnected } from "../../rpc/rpcFailure";
import { AppShell } from "../shell/AppShell";
import { TooltipProvider } from "../ui/tooltip";
import { SessionDrawer } from "./SessionDrawer";
import { SessionMainPane } from "./SessionMainPane";
import { HostStatsFooter } from "./HostStatsFooter";
import { useSessionAttachment } from "./useSessionAttachment";
import { sessionsDrawerPathForSession, SESSIONS_DRAWER_ROUTE, SESSIONS_NEW_ROUTE } from "../../routing/appRoutes";
import { detectIsMobile, useIsMobile } from "../../hooks/useIsMobile";
import { resolveShortcutsForSession } from "../../lib/toolShortcuts";
import { isCliTerminalSession } from "../../constants/claudeCliModels";
import { useSessionsDrawerInspector } from "./useSessionsDrawerInspector";
import { useSessionsDrawerClients } from "./useSessionsDrawerClients";
import { useProjectHomeSetup } from "./useProjectHomeSetup";
import { ProjectHomeSetupPane } from "./ProjectHomeSetupPane";
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
import {
  SessionsDrawerLoadNotices,
  SessionsDrawerPaneError,
  sessionsDrawerListNotices,
} from "./SessionsDrawerNotices";

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

  const {
    client,
    projectClient,
    worktreeClient,
    terminalClient,
    sessionFilesClient,
    sessionAgentClient,
    activityClient,
    catalogClient,
    accountsClient,
    execToolClient,
    prStackClient,
    taskClient,
  } = useSessionsDrawerClients();

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

  const {
    activeClient,
    activeWorktreeClient,
    activeCodeNavigationClient,
    activeTerminalClient,
    activeSessionFilesClient,
    activeSessionAgentClient,
    activeActivityClient,
    activeCatalogClient,
    activeExecToolClient,
    activePrStackClient,
    activeHost,
  } = useSessionsDrawerOwningHost(selectedSession, selectedInstanceId);

  // A daemon instance id as a host is named in text (see `rpc/hostLabel`) — the owning-host badge on
  // cross-host rows, and every "not connected" this screen says.
  const hostLabelForInstance = useHostLabel();

  const { paneError, reportAttachFailure, handleResume, handleDelete, handleTerminate } =
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
    });

  // Key-press shortcuts for the connected session's tool (shown as the mobile overlay).
  const mobileShortcuts = useMemo(
    () =>
      resolveShortcutsForSession(
        isCliTerminalSession(selectedSession?.agent ?? ""),
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
      {/* 100dvh (via AppShell fullbleed): on mobile 100vh includes the area behind the browser
          chrome, which would push the bottom keyboard bar off the visible screen. */}
      <AppShell
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
              selectedSession={selectedSession}
              attachment={attachment}
              attachmentHint={attachmentHint}
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
              projectClient={projectClient ?? undefined}
              taskClient={mode === "creating" ? (taskClient ?? undefined) : undefined}
              worktreeClient={
                mode === "creating"
                  ? (worktreeClient ?? undefined)
                  : (activeWorktreeClient ?? worktreeClient ?? undefined)
              }
              codeNavigationClient={activeCodeNavigationClient ?? undefined}
              terminalClient={
                mode === "creating"
                  ? (terminalClient ?? undefined)
                  : (activeTerminalClient ?? terminalClient ?? undefined)
              }
              sessionFilesClient={
                mode === "creating"
                  ? (sessionFilesClient ?? undefined)
                  : (activeSessionFilesClient ?? sessionFilesClient ?? undefined)
              }
              sessionAgentClient={
                mode === "creating"
                  ? (sessionAgentClient ?? undefined)
                  : (activeSessionAgentClient ?? sessionAgentClient ?? undefined)
              }
              activityClient={
                mode === "creating"
                  ? (activityClient ?? undefined)
                  : (activeActivityClient ?? activityClient ?? undefined)
              }
              accountsClient={accountsClient ?? undefined}
              catalogClient={
                mode === "creating"
                  ? (catalogClient ?? undefined)
                  : (activeCatalogClient ?? catalogClient ?? undefined)
              }
              execToolClient={
                mode === "creating"
                  ? (execToolClient ?? undefined)
                  : (activeExecToolClient ?? execToolClient ?? undefined)
              }
              prStackClient={
                mode === "creating"
                  ? (prStackClient ?? undefined)
                  : (activePrStackClient ?? prStackClient ?? undefined)
              }
              host={activeHost}
              sessionToken={sessionToken}
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
        <HostStatsFooter
          attachment={attachment}
          runtimes={runtimes}
          runtimeRegistry={runtimeRegistry}
        />
      </AppShell>
    </TooltipProvider>
    </UploadProgressProvider>
  );
}
