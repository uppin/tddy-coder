import React from "react";
import { projectForUnscopedSession } from "../../utils/sessionProjectTable";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { canResumeSession, sessionBaseViewMode } from "./sessionBaseView";
import { DormantSessionRuntime } from "./DormantSessionRuntime";
import { DormantAgentTerminal } from "./DormantAgentTerminal";
import { useAppLocation } from "../../routing/useAppLocation";
import { SessionWithActivitySidebar } from "./SessionWithActivitySidebar";
import { Button } from "tddy-ui-web";
import { CreateSessionPane } from "tddy-session-create-web";
import { AgentPicker } from "./AgentPicker";
import { SessionRuntime } from "./SessionRuntime";
import { resolveWorkflowView } from "./workflowViews";
import { SessionIndexingIndicator } from "../session/SessionIndexingIndicator";
import { WorktreeCodePane } from "../session/WorktreeCodePane";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import type { SessionMainPaneProps } from "./sessionMainPaneProps";
import { useSessionAgentConversations } from "./useSessionAgentConversations";
import {
  SessionAgentConversationLayer,
  useConversationHosts,
} from "./SessionAgentConversationLayer";
import { joinQuotedPaths } from "tddy-terminal-web";
import { PARAM_CODE } from "tddy-client-web";

export function SessionMainPane({
  selectedSession,
  attachment,
  attachmentHint = null,
  inspectorState,
  onToggleInspector,
  onInspectorClose,
  onInspectorExpand,
  onInspectorRestore,
  onResume,
  onResumeSession,
  onDelete,
  onTerminate,
  isCreating = false,
  client,
  projectClient,
  taskClient,
  worktreeClient,
  codeNavigationClient,
  terminalClient,
  sessionFilesClient,
  sessionAgentClient,
  activityClient,
  catalogClient,
  accountsClient,
  execToolClient,
  prStackClient,
  host,
  sessionToken = "",
  callerLogin = null,
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
  onSessionRegisterInsert,
  onInsertPathIntoTerminal,
  onSessionDisconnect,
  onSessionBytes,
  buildSessionExecToolClient,
  buildSessionActivityClient,
  sessionMetadataBySessionId,
}: SessionMainPaneProps) {
  const isConnected = attachment.status === "connected";

  // The worktree Code pane is a split view available for every session type: it never replaces the
  // base view (terminal / chat / PR-Stack), it opens beside it. Its open/closed state lives in the
  // URL (`?code=1`) so a shared link reproduces the pane layout.
  const { location, setParams } = useAppLocation();
  const codeOpen = location.params[PARAM_CODE] === "1";
  const toggleCodePane = React.useCallback(
    () => setParams({ [PARAM_CODE]: codeOpen ? null : "1" }),
    [codeOpen, setParams],
  );
  const codePaneEnabled = Boolean(worktreeClient && selectedSession);

  // The worktree RPCs require a non-empty `project_id`. Scoped sessions carry their own; unscoped
  // sessions (empty `projectId`) resolve to the registered project whose main repo is the longest
  // prefix of the session's `repoPath`.
  const resolvedProjectId = React.useMemo(() => {
    if (!selectedSession) return "";
    if ((selectedSession.projectId ?? "").trim() !== "") return selectedSession.projectId;
    return projectForUnscopedSession(selectedSession, [...projects])?.projectId ?? "";
  }, [selectedSession, projects]);

  const {
    agentConversations,
    activeConversations,
    pickerOpen,
    setPickerOpen,
    focusConversation,
    attachAgent,
    closeConversation,
  } = useSessionAgentConversations({ sessionAgentClient, selectedSession, sessionToken });

  // Where each session's conversation bodies live. They are rendered once, by the layer in the base
  // view below, and adopted into whichever tab strip shows their session — so neither the live
  // strip nor a stopped session's can end a conversation by going away. Only closing a tab does.
  const conversationHostFor = useConversationHosts();

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
        projectClient,
        catalogClient,
        prStackClient,
        sessionFilesClient,
        worktreeClient,
        taskClient,
        sessionToken,
        attachmentHint,
        sessions: [...sessions],
        defaultBranch,
        defaultRemote,
        onChildSessionStarted,
        // The peer switcher is `SessionsDrawerScreen`'s own `handleSelectSession` — select and
        // attach, reusing the runtime registry — which is exactly what opening a planned PR's bound
        // child session means, so the two share one handler rather than adding a second.
        onOpenSession: onSwitchPeer,
        sessionMetadataBySessionId,
      })
    : null;

  const hasRuntimes = runtimes.length > 0;

  // Which surface owns the pane below the top bar, and whether this session can be brought back.
  // Both are derived from the session's liveness (see `sessionBaseView`), so a session that is
  // resumed elsewhere corrects its own view on the next list poll — there is no view state to reset.
  const baseViewMode = sessionBaseViewMode(selectedSession, customView !== null);
  const dormant = canResumeSession(selectedSession);
  // Whether the stopped session's own tab strip is what shows (it covers that session's live strip).
  const dormantStripShown = baseViewMode === "activities" && selectedSession !== null;

  // The Files tab's Insert reaches the live runtime through the registry; a stopped session has no
  // runtime there, so its strip hands over the function that types into its focused bash tab.
  const dormantInsertRef = React.useRef<((text: string) => void) | null>(null);
  const registerDormantInsert = React.useCallback(
    (insert: ((text: string) => void) | null) => {
      dormantInsertRef.current = insert;
    },
    [],
  );
  const insertPathIntoTerminal = dormantStripShown
    ? (hostPath: string) => dormantInsertRef.current?.(joinQuotedPaths([hostPath]))
    : onInsertPathIntoTerminal;

  // One mounted terminal per attached session: the focused runtime's terminal is CSS-visible while
  // backgrounded ones are `display:none` but stay mounted (and subscribed to their LiveKit room), so
  // switching focus back is instant and background sessions keep streaming. Each runtime owns its
  // own terminal-control lease (see `SessionRuntime`), so the focused one carries the
  // `sessions-detail-terminal-container` marker (existing acceptance contract) and the
  // terminal-control claim toast. Extracted into a variable because a dormant session renders its tab
  // strip *over* this layer rather than instead of it.
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
          conversationHost={conversationHostFor(r.sessionId)}
          hostsConversations={!(dormantStripShown && r.sessionId === selectedSession?.sessionId)}
          focused={!dormant && r.sessionId === focusedRuntimeId}
          sessionToken={sessionToken}
          client={sessionAgentClient}
          terminalClient={terminalClient}
          mobileShortcuts={mobileShortcuts}
          onSessionRegisterInsert={onSessionRegisterInsert}
          onSessionDisconnect={onSessionDisconnect}
          onSessionBytes={onSessionBytes}
          host={host}
          sessions={sessions}
        />
      ))}
    </div>
  );

  // The base view (custom workflow view / a dormant session's tab strip / mounted terminals /
  // placeholder). Rendered on its own when the Code pane is closed, or as the left panel of the split
  // when it is open — never unmounted between the two so terminals stay attached.
  // What covers the runtimes, when anything does: a custom per-workflow view, or a dormant session's
  // own tab strip, whose Agent tab holds the recorded transcript. Null means the runtimes themselves
  // are the pane.
  //
  // The strip is keyed by session because its terminal list and lease belong to one stopped session:
  // a selection change must not carry one session's tabs into another's.
  const overlay: React.ReactNode =
    customView ??
    (dormantStripShown && selectedSession ? (
      <DormantSessionRuntime
        key={selectedSession.sessionId}
        session={selectedSession}
        sessionToken={sessionToken}
        terminalClient={terminalClient}
        mobileShortcuts={mobileShortcuts}
        onRegisterInsert={registerDormantInsert}
        conversationHost={conversationHostFor(selectedSession.sessionId)}
        agentTab={
          <DormantAgentTerminal
            sessionId={selectedSession.sessionId}
            sessionToken={sessionToken}
            terminalClient={terminalClient}
            onResume={() => onResumeSession(selectedSession.sessionId)}
          />
        }
        agentConversations={agentConversations[selectedSession.sessionId] ?? []}
        activeAgentConversationId={activeConversations[selectedSession.sessionId] ?? null}
        onSelectAgentConversation={(conversationId) =>
          focusConversation(selectedSession.sessionId, conversationId)
        }
        onCloseAgentConversation={(conversationId) =>
          closeConversation(selectedSession.sessionId, conversationId)
        }
      />
    ) : null);

  // The base view: the mounted runtimes, with an overlay over them when one owns the pane.
  //
  // The runtime layer sits in the **same slot in the element tree in every case** — shown, hidden
  // behind an overlay, or absent only because nothing is attached. That is load-bearing, not tidiness:
  // React unmounts a subtree that changes position, and unmounting a runtime unmounts the bodies of
  // its open agent conversations, each of which cancels its conversation on the daemon as it goes.
  // Selecting a workflow session used to do exactly that — swapping the layer out for the workflow's
  // own view — so an operator who glanced at a PR-Stack session came back to tabs whose conversations
  // the daemon had already dropped. Only closing a tab may end a conversation.
  //
  // Rendered on its own when the Code pane is closed, or as the left panel of the split when it is
  // open — never unmounted between the two, so terminals stay attached.
  const baseView = (
    <div className="flex-1 min-h-0 relative overflow-hidden">
      <SessionAgentConversationLayer
        agentConversations={agentConversations}
        activeConversations={activeConversations}
        hostFor={conversationHostFor}
        sessionToken={sessionToken}
        callerLogin={callerLogin}
        client={sessionAgentClient}
      />
      {hasRuntimes && (
        <div className={overlay !== null ? "hidden" : "absolute inset-0 flex flex-col"}>
          {runtimeLayer}
        </div>
      )}
      {overlay !== null && <div className="absolute inset-0 flex flex-col">{overlay}</div>}
      {!hasRuntimes &&
        overlay === null &&
        (isConnected ? (
          // Connected but the runtime hasn't been registered yet (brief window before the attach
          // effect runs) — keep the terminal container marker so existing acceptance contracts hold
          // during the transition.
          <div
            data-testid="sessions-detail-terminal-container"
            className="absolute inset-0 flex flex-col overflow-hidden"
          />
        ) : (
          // Disconnected / idle — simple placeholder
          <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
            Select Resume to reconnect
          </div>
        ))}
    </div>
  );

  return (
    <div
      data-testid="sessions-detail-pane"
      className="flex-1 min-w-0 flex flex-col h-full overflow-hidden relative"
    >
      {isCreating && client && projectClient && catalogClient && sessionFilesClient && worktreeClient && (
        <CreateSessionPane
          client={client}
          projectClient={projectClient}
          catalogClient={catalogClient}
          accountsClient={accountsClient}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          taskClient={taskClient}
          sessionToken={sessionToken}
          onCancel={onCancelCreate ?? (() => undefined)}
          onCreated={onSessionCreated ?? (() => undefined)}
        />
      )}

      {!isCreating && (
        <>
          {/* Header toggles — always visible when a session is selected */}
          {selectedSession ? (
            <SessionWithActivitySidebar
              session={selectedSession}
              sessionToken={sessionToken}
              client={buildSessionActivityClient?.() ?? activityClient}
              suppressed={baseViewMode === "activities"}
              toolbar={(toggle) => (
                <div className="flex items-center justify-end gap-1 px-2 py-1 border-b border-border flex-shrink-0">
                  {codeNavigationClient && (
                    <SessionIndexingIndicator
                      client={codeNavigationClient}
                      sessionToken={sessionToken}
                      sessionId={selectedSession.sessionId}
                    />
                  )}
                  {toggle}
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
            >
            {pickerOpen && sessionAgentClient && (
              <div className="flex-shrink-0 border-b border-border px-2 py-2">
                <AgentPicker
                  testIdPrefix="session-agent-picker"
                  errorTestId="session-agent-attach-error"
                  onAttach={attachAgent}
                  onClose={() => setPickerOpen(false)}
                />
              </div>
            )}

              {/* The Inspector overlay is available regardless of which base view is showing below
                  it — a custom per-workflow view (e.g. PR-Stack Chat Screen) only replaces the
                  terminal, it does not replace the Inspector. */}
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
                  {codeOpen && codePaneEnabled && worktreeClient && (
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
                          client={worktreeClient}
                          sessionToken={sessionToken}
                          projectId={resolvedProjectId}
                          worktreePath={selectedSession.repoPath}
                          navigationClient={codeNavigationClient}
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
                  sessions={sessions}
                  onClose={onInspectorClose}
                  onExpand={onInspectorExpand}
                  onRestore={onInspectorRestore}
                  onResume={onResume}
                  onDelete={onDelete}
                  onTerminate={onTerminate}
                  client={client}
                  execToolClient={execToolClient}
                  worktreeClient={worktreeClient}
                  sessionFilesClient={sessionFilesClient}
                  sessionToken={sessionToken}
                  /* The media tabs are gated on the connection to the host that owns this session —
                     `host`, the same connection the screen already resolved for every other
                     cross-host read, so the tabs and the RPCs behind them cannot disagree about
                     which daemon they are talking to.

                     The *host* is the upstream fact, not the session: `capabilitiesForHint`
                     (`rpc/connections/sessionAttachment.ts`) derives a session's capabilities from
                     whether its attach hint names a room, and whether there is a room at all is
                     decided by how the host is reached — a host reached without LiveKit can never
                     hand out a room-backed session.

                     Reading the session's own connection instead would also answer "no media" for a
                     *dormant* session, which has no connection at all. That turns an unanswerable
                     question into a refusal and hides tabs whose capability is in fact present,
                     which is a behaviour change rather than gating. */
                  hostConnection={host}
                  room={room}
                  serverIdentity={attachmentHint?.serverIdentity}
                  traffic={traffic}
                  buildSessionExecToolClient={buildSessionExecToolClient}
                  onInsertPathIntoTerminal={insertPathIntoTerminal}
                  onSwitchPeer={onSwitchPeer}
                  activityClient={buildSessionActivityClient?.() ?? activityClient}
                />
              </div>
            </SessionWithActivitySidebar>
          ) : (
            // No session selected
            <div className="flex items-center justify-center flex-1 text-muted-foreground text-sm">
              Select a session
            </div>
          )}
        </>
      )}
    </div>
  );
}
