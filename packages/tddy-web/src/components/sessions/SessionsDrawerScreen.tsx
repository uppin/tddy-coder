import React, { useCallback, useEffect, useMemo, useState } from "react";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { ProjectService } from "../../gen/project_pb";
import { SessionService, SessionEntrySchema, Signal } from "../../gen/session_pb";
import { ActivityService } from "../../gen/activity_pb";
import { AccountsService } from "../../gen/accounts_pb";
import { CatalogService } from "../../gen/catalog_pb";
import { ExecToolService } from "../../gen/exec_tools_pb";
import { PrStackService } from "../../gen/pr_stack_pb";
import { SessionAgentService } from "../../gen/session_agents_pb";
import { SessionFilesService } from "../../gen/session_files_pb";
import { TerminalSessionService } from "../../gen/terminal_session_pb";
import { WorktreeService } from "../../gen/worktree_pb";
import { useHostConnector } from "../../rpc/connections/registry";
import { useDaemonClient, useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useHostPresence } from "../../rpc/hostDirectory/useHostPresence";
import { UploadProgressProvider } from "../../rpc/uploadProgress";
import { requestSessionsRefresh } from "../../lib/sessionsRefreshBridge";
import { sessionNotificationRegistry } from "./sessionNotificationRegistry";
import { useSessionNotifications } from "../../rpc/useSessionNotifications";
import { useAuthContext } from "../../hooks/authProvider";
import { useActionFailure } from "../../hooks/useActionFailure";
import { hostNotConnected, rpcFailureText } from "../../rpc/rpcFailure";
import { owningHostForSession } from "../../utils/crossHostSessions";
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
import { useSessionsDrawerAttachClaims } from "./useSessionsDrawerAttachClaims";
import { useSessionsDrawerProjects } from "./useSessionsDrawerProjects";
import { InlineError } from "../ui/inline-error";
import { useSessionsDrawerRuntimeRegistry } from "./useSessionsDrawerRuntimeRegistry";
import { useSessionsDrawerSessionList } from "./useSessionsDrawerSessionList";
import { useSessionsDrawerRuntimeAttachment } from "./useSessionsDrawerRuntimeAttachment";
import { useSessionsDrawerOwningHost } from "./useSessionsDrawerOwningHost";
import { useSessionsDrawerActivation } from "./useSessionsDrawerActivation";
import { useSessionsDrawerDeleteSelection } from "./useSessionsDrawerDeleteSelection";
import { SessionsDrawerOpenListButton, SessionsDrawerUnknownSession } from "./SessionsDrawerScreenParts";

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

  // SessionService is daemon-level RPC — routed to whichever daemon is currently selected over
  // whichever wire reaches it (see `SelectedDaemonProvider` and `rpc/connections`).
  // `null` until a daemon is selected and a wire reaches it; every call site below guards.
  // The selected-daemon `client` still owns the CREATE flow (a new session is created on the
  // selected host); cross-host interaction routes through `activeClient` (computed below).
  const client = useDaemonClient(SessionService);
  const projectClient = useDaemonClient(ProjectService);
  // The worktree RPCs left `SessionService` for `worktree.WorktreeService`, so they need their
  // own client — the same daemon, over the same wire, addressed under the service that now serves
  // them. It follows `client`'s routing exactly (selected daemon here, owning host in
  // `activeWorktreeClient` below) so the pane and its worktree reads cannot disagree about which
  // host they are talking to.
  const worktreeClient = useDaemonClient(WorktreeService);
  // Likewise for the nine terminal RPCs, which left `SessionService` for
  // `terminal_session.TerminalSessionService`. Same daemon, same wire, and the same routing as
  // `client` — a runtime's control lease and its terminal I/O must reach the host the session's
  // PTY actually lives on.
  const terminalClient = useDaemonClient(TerminalSessionService);
  // And for the thirteen file RPCs, which left it for `session_files.SessionFilesService`: the
  // inspector's Files tab and the create form's attachment staging.
  const sessionFilesClient = useDaemonClient(SessionFilesService);
  // `#unbundle` node 7 took two more families out. The roster and conversation RPCs went to
  // `session_agents.SessionAgentService` — the Add-agent flow and every conversation pane — and
  // activity, notifications and ACP replay to `activity.ActivityService`. Same daemon, same wire
  // and the same routing as `client`, for the same reason the three above follow it.
  const sessionAgentClient = useDaemonClient(SessionAgentService);
  const activityClient = useDaemonClient(ActivityService);
  // `#unbundle` node 8 took families A, L and P to their own services on the same daemon and wire.
  const catalogClient = useDaemonClient(CatalogService);
  // The signed-in person's provider accounts live in this daemon's vault; the New-session form lists
  // them for the harness it is about to start.
  const accountsClient = useDaemonClient(AccountsService);
  const execToolClient = useDaemonClient(ExecToolService);
  const prStackClient = useDaemonClient(PrStackService);

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
  const daemons = useDaemons();
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

  const { projects, error: projectsError } = useSessionsDrawerProjects(projectClient, sessionToken);

  // Default closed on mobile (the open 280px panel would cover the main pane);
  // open on desktop.
  const [sessionListOpen, setSessionListOpen] = useState(() => !detectIsMobile());
  const isMobile = useIsMobile();

  const { state: attachment, hint: attachmentHint, connectSession, resumeSession, deleteSession, signalSession, restore: restoreAttachment, reset: resetAttachment } = useSessionAttachment();

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

  // Human-readable host label for a daemon instance id, with the local daemon's " (this daemon)"
  // suffix stripped — used for the owning-host badge on cross-host rows.
  const hostLabelForInstance = useCallback(
    (instanceId: string): string => {
      const host = daemons.find((d) => d.instanceId === instanceId);
      return (host?.label ?? instanceId).replace(/ \(this daemon\)$/, "");
    },
    [daemons],
  );

  // Why the selected session's last pane action failed — an attach, Resume, Delete or Terminate
  // that could not reach the session's owning host, said with the host named. A failure the daemon
  // itself reported lands on the attachment's `error` state instead; both render in one place below.
  const paneAction = useActionFailure();
  const { report: reportPaneFailure, clear: clearPaneFailure } = paneAction;
  const owningHostLabel = useCallback(
    () =>
      hostLabelForInstance(
        selectedSession ? owningHostForSession(selectedSession, selectedInstanceId ?? "") : (selectedInstanceId ?? ""),
      ),
    [hostLabelForInstance, selectedSession, selectedInstanceId],
  );
  const reportAttachFailure = useCallback(
    (err: unknown) => reportPaneFailure(new Error(`Attach failed: ${rpcFailureText(err)}`)),
    [reportPaneFailure],
  );
  // A pane failure is about the session it happened on, and is settled by that session attaching.
  useEffect(() => clearPaneFailure(), [selectedSessionId, clearPaneFailure]);
  useEffect(() => {
    if (attachment.status === "connected") clearPaneFailure();
  }, [attachment.status, clearPaneFailure]);
  const paneError =
    paneAction.error ?? (attachment.status === "error" ? attachment.error : null);

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

  const handleResume = (sessionId: string) => {
    clearPaneFailure();
    if (!activeHost) {
      reportPaneFailure(new Error(`Resume failed: ${rpcFailureText(hostNotConnected(owningHostLabel()))}`));
      return;
    }
    // `ResumeSession` returns the session's routing and `useSessionAttachment` puts the screen
    // straight into `connected` — the attach for this live epoch is already done.
    // Claiming it here stops the liveness effect from firing a second `ConnectSession` when the next
    // list poll reports the session alive, which would mint a fresh browser identity and bounce the
    // terminal through a reconnect and another `ClaimTerminalControl`. The claim is stamped with the
    // snapshot the operator acted on, so the dormant readings still in flight cannot revoke it.
    attachClaimRef.current = { sessionId, listGeneration };
    // `resumeSession` never rejects: a refused resume is the attachment's `error` state, shown below.
    void resumeSession(sessionId, sessionToken, activeHost).then((attached) => {
      // The resume never landed, so no attach was taken after all — release the claim so the
      // liveness effect can still attach if the session turns out to be alive.
      if (!attached) attachClaimRef.current = null;
    });
  };

  const handleDelete = (sessionId: string) => {
    clearPaneFailure();
    if (!activeClient) {
      reportPaneFailure(new Error(`Delete failed: ${rpcFailureText(hostNotConnected(owningHostLabel()))}`));
      return;
    }
    // Never rejects — a refused delete is the attachment's `error` state.
    void deleteSession(sessionId, sessionToken, activeClient);
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
    client,
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

  const handleTerminate = (sessionId: string) => {
    clearPaneFailure();
    if (!activeClient) {
      reportPaneFailure(new Error(`Terminate failed: ${rpcFailureText(hostNotConnected(owningHostLabel()))}`));
      return;
    }
    // Never rejects — a refused signal is the attachment's `error` state. Common cause: the session
    // already ended (e.g. process exited on its own) before this click reached the daemon — the
    // refresh below still corrects the stale `isActive` that showed "Terminate" for a dead session.
    void signalSession(sessionId, Signal.SIGTERM, sessionToken, activeClient)
      .finally(() => {
        // The daemon computes `isActive` from live PID liveness, not from a push update — refetch
        // so the row (and its "Terminate" button) reflects the session's actual current state.
        requestSessionsRefresh();
      });
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
        {/* Screen-wide reads that failed: the registry unscoped sessions resolve their project from,
            and the feed the row dots follow. Quiet, one line each, gone once the read recovers. */}
        {(projectsError || notificationsError) && (
          <div className="flex-shrink-0 px-3 py-1 border-b border-border space-y-0.5">
            <InlineError
              testId="sessions-drawer-projects-load-error"
              message={projectsError && `Projects unavailable: ${projectsError}`}
              className="text-xs"
            />
            <InlineError
              testId="sessions-drawer-notifications-error"
              message={notificationsError && `Session notifications unavailable: ${notificationsError}`}
              className="text-xs"
            />
          </div>
        )}
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
            notices={
              listLoadError || bulkDeleteFailures.length > 0 ? (
                <>
                  <InlineError testId="sessions-drawer-load-error" message={listLoadError} className="text-xs" />
                  {bulkDeleteFailures.map((f) => (
                    <InlineError
                      key={f.sessionId}
                      testId={`sessions-drawer-bulk-delete-error-${f.sessionId}`}
                      message={`Could not delete ${f.label}: ${f.reason}`}
                      className="text-xs"
                    />
                  ))}
                </>
              ) : undefined
            }
          />
          {/* A bad deep link surfaces "not found" in the detail pane only — the session list
              stays visible so the operator can pick a valid session. */}
          {/* The selected session's last failed action, over its pane: what the operator just tried
              on this session, and why it did not work. */}
          {!unknownSession && paneError && (
            <InlineError
              testId="sessions-pane-error"
              message={paneError}
              className="absolute bottom-3 left-1/2 z-40 max-w-[min(90%,40rem)] -translate-x-1/2 rounded-md border border-destructive/40 bg-background px-3 py-1.5 shadow-md"
            />
          )}
          {unknownSession ? (
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
