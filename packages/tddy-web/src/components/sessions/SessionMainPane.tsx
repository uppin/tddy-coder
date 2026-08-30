import React from "react";
import { ConnectError, type Client, type Transport } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { ConnectionService, SessionEntry, ProjectEntry } from "../../gen/connection_pb";
import { projectForUnscopedSession } from "../../utils/sessionProjectTable";
import type { TokenService } from "../../gen/token_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { canResumeSession, sessionBaseViewMode } from "./sessionBaseView";
import { SessionActivitiesPane } from "./SessionActivitiesPane";
import { PARAM_CODE } from "../../routing/appLocation";
import { useAppLocation } from "../../routing/useAppLocation";
import { AgentActivityOverlay } from "./AgentActivityOverlay";
import { Button } from "../ui/button";
import { CreateSessionPane } from "./CreateSessionPane";
import { AgentPicker } from "./AgentPicker";
import type { AvailableAgent } from "./useAvailableAgents";
import {
  conversationForAgent,
  withAgentConversation,
  type AgentConversation,
} from "./agentConversationTabs";
import { randomUuid } from "../../lib/randomId";
import { SessionAgentsSection } from "./SessionAgentsSection";
import { SessionRuntime } from "./SessionRuntime";
import { sessionPeers } from "../../utils/sessionPeers";
import { resolveWorkflowView } from "./workflowViews";
import { WorktreeCodePane } from "../session/WorktreeCodePane";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { ByteDelta, SessionRuntimeState } from "./sessionRuntimeRegistry";

type ConnectionClient = Client<typeof ConnectionService>;
type TokenClient = Client<typeof TokenService>;

interface SessionMainPaneProps {
  selectedSession: SessionEntry | null;
  attachment: SessionAttachmentState;
  inspectorState: InspectorDrawerState;
  onToggleInspector: () => void;
  onInspectorClose: () => void;
  onInspectorExpand: () => void;
  onInspectorRestore: () => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
  // Create session mode
  isCreating?: boolean;
  client?: ConnectionClient;
  /** Client for fetching browser LiveKit tokens — required to render a terminal for `connected-livekit` sessions. */
  tokenClient?: TokenClient;
  sessionToken?: string;
  onCancelCreate?: () => void;
  onSessionCreated?: (sessionId: string) => void;
  /** LiveKit room for the connected session (used by VNC / screen-sharing overlay and as the
   *  common-room stand-in for session-scoped RPCs when the transport factory is overridden). */
  room?: Room | null;
  /** Shortcut presets for the connected session — shown as the mobile shortcut overlay. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Fired when a custom workflow view (e.g. PrStackScreen) spawns a child session. */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
  /** Fired when the operator clicks a peer's "Switch" button in the Session agents section — the
   *  parent selects that peer session in the drawer (focuses its runtime). No transport of its own. */
  onSwitchPeer?: (sessionId: string) => void;
  /** Inspector I/O traffic (req 5 dual source): live runtime counters for active sessions,
   *  daemon-sourced `SessionEntry` fields for inactive / non-LiveKit sessions. */
  traffic?: { bytesIn: number; bytesOut: number; lastDataReceivedAt: number | null } | null;
  /** Project registry — used to resolve an unscoped session's project (empty `projectId`) from its
   *  `repoPath` before the worktree Code pane's RPCs, which require a non-empty `project_id`. */
  projects?: ReadonlyArray<ProjectEntry>;
  /** Attached runtimes — one mounted terminal per entry (focused visible, others hidden). */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The full drawer session list — passed to each runtime so it can render its spawned child
   *  conversations (`orchestratorSessionId === session`) as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
  /** The focused runtime's session id (visible); others are `display:none` but stay mounted. */
  focusedRuntimeId?: string | null;
  /** Capture a session's connected LiveKit `Room` so session-scoped RPCs can route over it. */
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Register a session's Agent-terminal text-insert (for the inspector Files-tab click/tap route). */
  onSessionRegisterInsert?: (sessionId: string, insertInput: (text: string) => void) => void;
  /** Insert an uploaded file's host path into the focused session's terminal (Files tab → Insert /
   *  tap), closing the inspector. */
  onInsertPathIntoTerminal?: (hostPath: string) => void;
  /** Evict a session's runtime terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** Fold a session's terminal I/O bytes into its runtime counters (inspector I/O meter). */
  onSessionBytes?: (sessionId: string, delta: ByteDelta) => void;
  /** Lazy builder for a session-scoped `ConnectionService` client (session-participant routing) —
   *  used by the inspector's session-scoped RPCs (e.g. ExecuteTool). */
  buildSessionClient?: () => ConnectionClient | null;
  /** LiveKit transport factory — passed through to each `SessionRuntime` for its explicit
   *  steal-claim (`ClaimTerminalControl`) session-participant routing. */
  liveKitFactory?: (room: Room, targetIdentity: string) => Transport;
  /** True when `liveKitFactory` is a test double that ignores its `room` argument. */
  liveKitFactoryIsOverridden?: boolean;
}

export function SessionMainPane({
  selectedSession,
  attachment,
  inspectorState,
  onToggleInspector,
  onInspectorClose,
  onInspectorExpand,
  onInspectorRestore,
  onResume,
  onDelete,
  onTerminate,
  isCreating = false,
  client,
  tokenClient,
  sessionToken = "",
  onCancelCreate,
  onSessionCreated,
  room = null,
  mobileShortcuts,
  onChildSessionStarted,
  onSwitchPeer,
  traffic,
  projects = [],
  runtimes = [],
  sessions = [],
  focusedRuntimeId = null,
  onSessionRoom,
  onSessionRegisterInsert,
  onInsertPathIntoTerminal,
  onSessionDisconnect,
  onSessionBytes,
  buildSessionClient,
  liveKitFactory,
  liveKitFactoryIsOverridden,
}: SessionMainPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  // The worktree Code pane is a split view available for every session type: it never replaces the
  // base view (terminal / chat / PR-Stack), it opens beside it. Its open/closed state lives in the
  // URL (`?code=1`) so a shared link reproduces the pane layout.
  const { location, setParams } = useAppLocation();
  const codeOpen = location.params[PARAM_CODE] === "1";
  const toggleCodePane = React.useCallback(
    () => setParams({ [PARAM_CODE]: codeOpen ? null : "1" }),
    [codeOpen, setParams],
  );
  const codePaneEnabled = Boolean(client && selectedSession);

  // The selected session's peers — child sessions whose `orchestratorSessionId` is this session.
  const peers = React.useMemo(
    () => (selectedSession ? sessionPeers([...sessions], selectedSession.sessionId) : []),
    [sessions, selectedSession],
  );

  // The worktree RPCs require a non-empty `project_id`. Scoped sessions carry their own; unscoped
  // sessions (empty `projectId`) resolve to the registered project whose main repo is the longest
  // prefix of the session's `repoPath`.
  const resolvedProjectId = React.useMemo(() => {
    if (!selectedSession) return "";
    if ((selectedSession.projectId ?? "").trim() !== "") return selectedSession.projectId;
    return projectForUnscopedSession(selectedSession, [...projects])?.projectId ?? "";
  }, [selectedSession, projects]);

  // The specialized agents attached to each session, and which of their conversations holds that
  // session's pane. Held here rather than inside `SessionRuntime` because a runtime is backgrounded
  // (not unmounted) on a session switch and must come back with its conversations intact — only
  // closing a tab ends one.
  const [agentConversations, setAgentConversations] = React.useState<
    Record<string, AgentConversation[]>
  >({});
  const [activeConversations, setActiveConversations] = React.useState<
    Record<string, string | null>
  >({});
  const [pickerOpen, setPickerOpen] = React.useState(false);

  const focusConversation = React.useCallback(
    (sessionId: string, conversationId: string | null) => {
      setActiveConversations((current) => ({ ...current, [sessionId]: conversationId }));
    },
    [],
  );

  /**
   * Attach the picked agent to the session on screen and open a conversation tab with it.
   *
   * The conversation id is minted here, by the browser, so the tab is keyed by an id it can also
   * cancel — `OpenAgentConversation` accepts a caller-chosen one precisely for that. It is minted
   * with `randomUuid`, never `crypto.randomUUID`, which is undefined on the plain-http LAN origins
   * this app is routinely served from. The `OpenAgentConversation` call itself belongs to the tab's
   * body, which owns the conversation for as long as the tab is open.
   */
  const attachAgent = async (agent: AvailableAgent): Promise<string | null> => {
    // The picker is only rendered with both in hand; a refusal names what is missing rather than
    // reporting an attach that was never sent as a success.
    if (!client || !selectedSession) return "Not connected to this session's daemon.";
    const { sessionId, daemonInstanceId } = selectedSession;
    try {
      await client.attachSessionAgent({
        sessionToken,
        sessionId,
        daemonInstanceId,
        agentId: agent.agentId,
      });
    } catch (err) {
      return ConnectError.from(err).rawMessage;
    }

    // Attaching an agent that is already attached is a no-op on the roster, so a second tab would
    // claim something the daemon did not do: the conversation already open is focused instead.
    const open = agentConversations[sessionId] ?? [];
    const existing = conversationForAgent(open, agent.agentId);
    const conversation: AgentConversation = existing ?? {
      conversationId: randomUuid(),
      agentId: agent.agentId,
      label: agent.label,
      daemonInstanceId,
    };
    if (existing === null) {
      setAgentConversations((current) => ({
        ...current,
        [sessionId]: withAgentConversation(current[sessionId] ?? [], conversation),
      }));
    }
    focusConversation(sessionId, conversation.conversationId);
    return null;
  };

  /** Drop a conversation's tab. Unmounting its body is what cancels the conversation. */
  const closeConversation = React.useCallback(
    (sessionId: string, conversationId: string) => {
      setAgentConversations((current) => ({
        ...current,
        [sessionId]: (current[sessionId] ?? []).filter(
          (c) => c.conversationId !== conversationId,
        ),
      }));
      setActiveConversations((current) => ({
        ...current,
        [sessionId]: current[sessionId] === conversationId ? null : (current[sessionId] ?? null),
      }));
    },
    [],
  );

  // The selected session's project default branch, read from the registry the drawer already loaded
  // rather than an RPC or a git probe (D20). Empty when the project is not in the list or stores no
  // `main_branch_ref` (a legacy project): the PR-Stack view then labels the base "default branch" and
  // the daemon resolves the real ref when it is asked to act.
  const projectForSession = React.useMemo(
    () => projects.find((p) => p.projectId === resolvedProjectId),
    [projects, resolvedProjectId],
  );
  const defaultBranch = React.useMemo(
    () => projectForSession?.mainBranchRef ?? "",
    [projectForSession],
  );
  // The project's resolved default remote (`origin`, `upstream`, ...). Empty for a legacy project that
  // stored none — the PR-Stack view falls back to `origin` (the daemon's own last resort) when lifting
  // a stack node's local branch name into the `<remote>/<branch>` ref the daemon fetches.
  const defaultRemote = React.useMemo(
    () => projectForSession?.defaultRemote ?? "",
    [projectForSession],
  );

  const customView = !isCreating
    ? resolveWorkflowView(selectedSession, {
        client,
        sessionToken,
        attachment,
        sessions: [...sessions],
        defaultBranch,
        defaultRemote,
        onChildSessionStarted,
        // The peer switcher is `SessionsDrawerScreen`'s own `handleSelectSession` — select and
        // attach, reusing the runtime registry — which is exactly what opening a planned PR's bound
        // child session means, so the two share one handler rather than adding a second.
        onOpenSession: onSwitchPeer,
      })
    : null;

  const hasRuntimes = runtimes.length > 0;

  // Which surface owns the pane below the top bar, and whether this session can be brought back.
  // Both are derived from the session's liveness (see `sessionBaseView`), so a session that is
  // resumed elsewhere corrects its own view on the next list poll — there is no view state to reset.
  const baseViewMode = sessionBaseViewMode(selectedSession, customView !== null);
  const dormant = canResumeSession(selectedSession);

  // One mounted terminal per attached session: the focused runtime's terminal is CSS-visible while
  // backgrounded ones are `display:none` but stay mounted (and subscribed to their LiveKit room), so
  // switching focus back is instant and background sessions keep streaming. Each runtime owns its
  // own terminal-control lease (see `SessionRuntime`), so the focused one carries the
  // `sessions-detail-terminal-container` marker (existing acceptance contract) and the
  // terminal-control mutex overlay. Extracted into a variable because a dormant session renders the
  // Activities view *over* this layer rather than instead of it.
  //
  // A dormant session never foregrounds a terminal: there is no live process behind it, so
  // foregrounding one would only put a stale screen (and its claim-terminal CTA) over the view the
  // operator came to read. That suppression used to key off the inspector being docked, which was
  // the same predicate by accident; it names its real cause now.
  const runtimeLayer = (
    <div data-testid="sessions-runtime-layer" className="flex-1 min-h-0 relative overflow-hidden">
      {runtimes.map((r) => (
        <SessionRuntime
          key={r.sessionId}
          runtime={r}
          agentConversations={agentConversations[r.sessionId] ?? []}
          activeAgentConversationId={activeConversations[r.sessionId] ?? null}
          onSelectAgentConversation={(conversationId) =>
            focusConversation(r.sessionId, conversationId)
          }
          onCloseAgentConversation={(conversationId) =>
            closeConversation(r.sessionId, conversationId)
          }
          focused={!dormant && r.sessionId === focusedRuntimeId}
          sessionToken={sessionToken}
          client={client}
          tokenClient={tokenClient}
          mobileShortcuts={mobileShortcuts}
          onSessionRoom={onSessionRoom}
          onSessionRegisterInsert={onSessionRegisterInsert}
          onSessionDisconnect={onSessionDisconnect}
          onSessionBytes={onSessionBytes}
          liveKitFactory={liveKitFactory}
          liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
          commonRoom={room}
          sessions={sessions}
        />
      ))}
    </div>
  );

  // The base view (custom workflow view / recorded activities / mounted terminals / placeholder).
  // Rendered on its own when the Code pane is closed, or as the left panel of the split when it is
  // open — never unmounted between the two so terminals stay attached.
  const baseView = customView ? (
    // Custom per-workflow view — renders in place of the terminal regardless of attachment
    // status; the workflow owns its own chrome.
    customView
  ) : baseViewMode === "activities" && selectedSession ? (
    // Dormant session — its recorded ACP transcript is the pane. A runtime attached earlier stays
    // mounted behind it (background streaming preserved, a later resume is instant) but unfocused,
    // so the transcript is what shows.
    <div className="flex-1 min-h-0 relative overflow-hidden">
      {hasRuntimes && <div className="absolute inset-0 flex flex-col">{runtimeLayer}</div>}
      <div className="absolute inset-0 flex flex-col">
        <SessionActivitiesPane
          sessionId={selectedSession.sessionId}
          sessionToken={sessionToken}
          client={buildSessionClient?.() ?? client}
        />
      </div>
    </div>
  ) : hasRuntimes ? (
    runtimeLayer
  ) : isConnected ? (
    // Connected but the runtime hasn't been registered yet (brief window before the attach
    // effect runs) — keep the terminal container marker so existing acceptance contracts hold
    // during the transition.
    <div
      data-testid="sessions-detail-terminal-container"
      className="flex-1 min-h-0 flex flex-col relative overflow-hidden"
    />
  ) : (
    // Disconnected / idle — simple placeholder
    <div className="flex-1 min-h-0 relative overflow-hidden">
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        Select Resume to reconnect
      </div>
    </div>
  );

  return (
    <div
      data-testid="sessions-detail-pane"
      className="flex-1 min-w-0 flex flex-col h-full overflow-hidden relative"
    >
      {isCreating && client && (
        <CreateSessionPane
          client={client}
          sessionToken={sessionToken}
          onCancel={onCancelCreate ?? (() => undefined)}
          onCreated={onSessionCreated ?? (() => undefined)}
        />
      )}

      {!isCreating && (
        <>
          {/* Header toggles — always visible when a session is selected */}
          {selectedSession && (
            <div className="flex items-center justify-end gap-1 px-2 py-1 border-b border-border flex-shrink-0">
              {/* One transcript per pane: the overlay replays exactly what the Activities view is
                  already showing, so it is suppressed there — and only there. It stays the only way
                  to read the transcript for an active session and for a dormant session whose base
                  view is a workflow screen. */}
              {baseViewMode !== "activities" && (
                <AgentActivityOverlay
                  sessionId={selectedSession.sessionId}
                  sessionToken={sessionToken}
                  sessionType={selectedSession.sessionType}
                  client={buildSessionClient?.() ?? client}
                />
              )}
              {/* Resume is keyed on liveness alone, so every dormant session offers it from the same
                  position — including the ones whose base view (PR-Stack, workflow chat) is left
                  alone. It calls the same handler as the inspector's own Resume. */}
              {dormant && (
                <Button
                  data-testid={`sessions-main-resume-${selectedSession.sessionId}`}
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => onResume(selectedSession.sessionId)}
                  title="Resume this session"
                >
                  Resume
                </Button>
              )}
              {/* Attaches a specialized agent from the roster catalog to THIS session and opens a
                  conversation tab with it. It does not spawn a peer session, which is what it used
                  to do. Only offered with a daemon client in hand — there is no attach without one. */}
              {client && (
                <Button
                  data-testid="session-agent-attach-btn"
                  variant="ghost"
                  size="sm"
                  className="h-6 px-2 text-xs"
                  onClick={() => setPickerOpen(true)}
                  title="Attach an agent to this session and talk to it"
                >
                  Add agent
                </Button>
              )}
              <Button
                data-testid="sessions-code-toggle"
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={toggleCodePane}
                title="Toggle worktree code pane"
              >
                Code
              </Button>
              <Button
                data-testid="sessions-inspector-toggle"
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={onToggleInspector}
                title="Toggle inspector"
              >
                Inspector
              </Button>
            </div>
          )}

          {selectedSession && pickerOpen && client && (
            <div className="flex-shrink-0 border-b border-border px-2 py-2">
              <AgentPicker
                testIdPrefix="session-agent-picker"
                errorTestId="session-agent-attach-error"
                onAttach={attachAgent}
                onClose={() => setPickerOpen(false)}
              />
            </div>
          )}

          {/* Session agents section — lists the selected session's peers (children via
              orchestratorSessionId). Always mounted when a session is selected so the empty state
              is consistent; the section itself renders an empty-state message when there are none. */}
          {selectedSession && (
            <SessionAgentsSection
              peers={peers}
              onSwitchPeer={(sessionId) => onSwitchPeer?.(sessionId)}
            />
          )}

          {!selectedSession ? (
            // No session selected
            <div className="flex items-center justify-center flex-1 text-muted-foreground text-sm">
              Select a session
            </div>
          ) : (
            // The Inspector overlay is available regardless of which base view is showing below
            // it — a custom per-workflow view (e.g. PR-Stack Chat Screen) only replaces the
            // terminal, it does not replace the Inspector.
            <div className="flex-1 min-h-0 flex flex-col relative overflow-hidden">
              {/* The base view always lives in the same `Panel` (stable id/order), whether or not
                  the Code pane is open, so toggling never re-mounts it — a live terminal keeps its
                  attachment and a chat keeps its LiveKit room. Opening the pane only adds the second
                  panel + resize handle. */}
              <PanelGroup direction="horizontal" className="flex-1 min-h-0">
                <Panel
                  id="session-base-view"
                  order={1}
                  minSize={25}
                  className="flex min-h-0 flex-col overflow-hidden"
                >
                  {baseView}
                </Panel>
                {codeOpen && codePaneEnabled && client && (
                  <>
                    <PanelResizeHandle className="w-1 bg-border transition-colors hover:bg-primary/40" />
                    <Panel
                      id="worktree-code-pane"
                      order={2}
                      defaultSize={40}
                      minSize={20}
                      className="flex min-h-0 flex-col overflow-hidden"
                    >
                      <WorktreeCodePane
                        client={client}
                        sessionToken={sessionToken}
                        projectId={resolvedProjectId}
                        worktreePath={selectedSession.repoPath}
                      />
                    </Panel>
                  </>
                )}
              </PanelGroup>
              {/* Inspector overlay — available for every base view above. Key is suffixed
                  (not just sessionId) because the customView branch above (e.g. PrStackScreen)
                  is keyed on sessionId too, and both are siblings here — an identical key would
                  collide. */}
              <SessionInspectorDrawer
                key={`inspector-${selectedSession.sessionId}`}
                state={inspectorState}
                session={selectedSession}
                onClose={onInspectorClose}
                onExpand={onInspectorExpand}
                onRestore={onInspectorRestore}
                onResume={onResume}
                onDelete={onDelete}
                onTerminate={onTerminate}
                client={client}
                sessionToken={sessionToken}
                room={room}
                serverIdentity={
                  attachment.status === "connected-livekit"
                    ? attachment.livekitServerIdentity
                    : undefined
                }
                traffic={traffic}
                buildSessionClient={buildSessionClient}
                onInsertPathIntoTerminal={onInsertPathIntoTerminal}
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
