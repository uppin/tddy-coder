import React from "react";
import { type Client, type Transport } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { TokenService } from "../../gen/token_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { Button } from "../ui/button";
import { CreateSessionPane } from "./CreateSessionPane";
import { SessionRuntime } from "./SessionRuntime";
import { resolveWorkflowView } from "./workflowViews";
import { WorktreeCodePane } from "../session/WorktreeCodePane";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { SessionRuntimeState } from "./sessionRuntimeRegistry";

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
  /** Inspector I/O traffic (req 5 dual source): live runtime counters for active sessions,
   *  daemon-sourced `SessionEntry` fields for inactive / non-LiveKit sessions. */
  traffic?: { bytesIn: number; bytesOut: number; lastDataReceivedAt: number | null } | null;
  /** Attached runtimes — one mounted terminal per entry (focused visible, others hidden). */
  runtimes?: ReadonlyArray<SessionRuntimeState>;
  /** The full drawer session list — passed to each runtime so it can render its spawned child
   *  conversations (`orchestratorSessionId === session`) as tabs. */
  sessions?: ReadonlyArray<SessionEntry>;
  /** The focused runtime's session id (visible); others are `display:none` but stay mounted. */
  focusedRuntimeId?: string | null;
  /** Capture a session's connected LiveKit `Room` so session-scoped RPCs can route over it. */
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Evict a session's runtime terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
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
  traffic,
  runtimes = [],
  sessions = [],
  focusedRuntimeId = null,
  onSessionRoom,
  onSessionDisconnect,
  buildSessionClient,
  liveKitFactory,
  liveKitFactoryIsOverridden,
}: SessionMainPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  // The worktree Code pane is a split view available for every session type: it never replaces the
  // base view (terminal / chat / PR-Stack), it opens beside it.
  const [codeOpen, setCodeOpen] = React.useState(false);
  const codePaneEnabled = Boolean(client && selectedSession);

  const customView = !isCreating
    ? resolveWorkflowView(selectedSession, {
        client,
        sessionToken,
        attachment,
        onChildSessionStarted,
      })
    : null;

  // The focused runtime's terminal is CSS-visible; backgrounded runtimes are `display:none` but
  // stay mounted (and subscribed to their LiveKit room) so switching focus back is instant and
  // background sessions keep streaming. Each runtime owns its own terminal-control lease (see
  // `SessionRuntime`), so the focused one carries the `sessions-detail-terminal-container` marker
  // (existing acceptance contract) and the terminal-control mutex overlay.
  const hasRuntimes = runtimes.length > 0;

  // The base view (custom workflow view / mounted terminals / placeholder). Rendered on its own
  // when the Code pane is closed, or as the left panel of the split when it is open — never
  // unmounted between the two so terminals stay attached.
  const baseView = customView ? (
    // Custom per-workflow view — renders in place of the terminal regardless of attachment
    // status; the workflow owns its own chrome.
    customView
  ) : hasRuntimes ? (
    // One mounted terminal per attached session (focused visible, others hidden). Each runtime
    // owns its terminal-control lease — see `SessionRuntime`.
    <div
      data-testid="sessions-runtime-layer"
      className="flex-1 min-h-0 relative overflow-hidden"
    >
      {runtimes.map((r) => (
        <SessionRuntime
          key={r.sessionId}
          runtime={r}
          focused={r.sessionId === focusedRuntimeId}
          sessionToken={sessionToken}
          client={client}
          tokenClient={tokenClient}
          mobileShortcuts={mobileShortcuts}
          onSessionRoom={onSessionRoom}
          onSessionDisconnect={onSessionDisconnect}
          liveKitFactory={liveKitFactory}
          liveKitFactoryIsOverridden={liveKitFactoryIsOverridden}
          commonRoom={room}
          sessions={sessions}
        />
      ))}
    </div>
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
            <div className="flex justify-end gap-1 px-2 py-1 border-b border-border flex-shrink-0">
              <Button
                data-testid="sessions-code-toggle"
                variant="ghost"
                size="sm"
                className="h-6 px-2 text-xs"
                onClick={() => setCodeOpen((open) => !open)}
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
                        projectId={selectedSession.projectId}
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
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
