import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import type { WorkQueueSummary } from "tddy-client-web/gen/work_queue_pb";
import {
  sessionsDrawerPathForSession,
  sessionsProjectNewPath,
  sessionsProjectSettingsPath,
  SESSIONS_DRAWER_ROUTE,
  SESSIONS_NEW_ROUTE,
} from "tddy-client-web";
import { hostNotConnected } from "tddy-client-web";
import type { HostConnection } from "tddy-client-web/connections/types";
import { requestSessionsRefresh } from "./lib/sessionsRefreshBridge";
import { useEnsureProjectHome } from "./useEnsureProjectHome";
import type { useProjectHomeSetup } from "./useProjectHomeSetup";

/**
 * The sessions drawer screen's actions (`#orchestrators` 12/12, B10): navigation and creation
 * handlers the list and the detail pane share. Split out of `SessionsDrawerScreen`.
 */
export function useSessionsDrawerScreenActions(input: {
  navigate: (path: string) => void;
  isMobile: boolean;
  setSessionListOpen: (open: boolean) => void;
  setUnknownSession: (unknown: boolean) => void;
  addOptimisticSession: (entry: import("tddy-client-web/gen/session_pb").SessionEntry) => void;
  selectedInstanceId: string | null;
  connectHost: (instanceId: string | null) => HostConnection | null;
  connectSession: (sessionId: string, token: string, host: HostConnection) => Promise<boolean>;
  settleAttach: (sessionId: string, attached: boolean) => void;
  awaitingCreatedSessionIdRef: { current: string | null };
  reportAttachFailure: (error: unknown) => void;
  hostLabelForInstance: (instanceId: string) => string;
  projectHomeSetup: ReturnType<typeof useProjectHomeSetup>;
  navigationPort: { navigate: (to: { path: string; params: Record<string, string> }) => void };
  client: Parameters<typeof useEnsureProjectHome>[0];
  sessionToken: string;
  handleSelectSession: (sessionId: string) => void;
  reloadProjects: () => void;
}) {
  const {
    navigate,
    isMobile,
    setSessionListOpen,
    setUnknownSession,
    addOptimisticSession,
    selectedInstanceId,
    connectHost,
    connectSession,
    settleAttach,
    awaitingCreatedSessionIdRef,
    reportAttachFailure,
    hostLabelForInstance,
    projectHomeSetup,
    navigationPort,
    client,
    sessionToken,
    handleSelectSession,
    reloadProjects,
  } = input;

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
  const handleNewOrchestrator = (projectId: string) => {
    if (isMobile) setSessionListOpen(false);
    navigate(sessionsProjectNewPath(projectId));
  };
  // A project's work queue opens in its owning session, whose work queue sidebar it opens on: a
  // queue's id is its session's (`#workqueue` 5/14).
  const handleOpenProjectStack = (summary: WorkQueueSummary) => {
    const owner = summary.address?.queueId;
    if (!owner) return;
    if (isMobile) setSessionListOpen(false);
    handleSelectSession(owner);
    navigationPort.navigate({ path: sessionsDrawerPathForSession(owner), params: {} });
  };
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
  // The gear opens the settings pane on the drawer's selected host: no `?host=` of its own, and
  // any stale one from a previous settings route is dropped.
  const handleOpenProjectSettings = (projectId: string) => {
    if (isMobile) setSessionListOpen(false);
    navigationPort.navigate({ path: sessionsProjectSettingsPath(projectId), params: {} });
  };
  const closeSettings = () => navigationPort.navigate({ path: SESSIONS_DRAWER_ROUTE, params: {} });
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

  return {
    handleUnknownHome,
    handleChildSessionStarted,
    handleCreateSession,
    handleNewOrchestrator,
    handleOpenProjectStack,
    handleCancelCreate,
    handleSessionCreated,
    openProjectHomeSetup,
    handleOpenProjectSettings,
    closeSettings,
    ensureProjectHome,
    startProjectHome,
  };
}
