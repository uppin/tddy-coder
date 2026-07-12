import React, { type MutableRefObject } from "react";
import type { Client } from "@connectrpc/connect";
import type { Room } from "livekit-client";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { TokenService } from "../../gen/token_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { Button } from "../ui/button";
import { CreateSessionPane } from "./CreateSessionPane";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import { SessionLiveKitTerminal } from "./SessionLiveKitTerminal";
import { resolveWorkflowView } from "./workflowViews";
import type { TerminalControlState } from "./terminalControlState";
import type { ToolShortcutDef } from "../../lib/toolShortcuts";
import type { SessionRuntimeState } from "./sessionRuntimeRegistry";
import { cn } from "../../lib/utils";

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
  // Terminal control state — when present and not the controller, renders a "Claim terminal" CTA.
  terminalControl?: TerminalControlState & { onClaim: () => void };
  /** Ref to the live control token from useTerminalControl. Passed through to GrpcSessionTerminal. */
  controlTokenRef?: MutableRefObject<string>;
  /** LiveKit room for the connected session (used by VNC / screen-sharing overlay). Null when no room is available. */
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
  /** The focused runtime's session id (visible); others are `display:none` but stay mounted. */
  focusedRuntimeId?: string | null;
  /** Capture a session's connected LiveKit `Room` so session-scoped RPCs can route over it. */
  onSessionRoom?: (sessionId: string, room: Room) => void;
  /** Evict a session's runtime terminal (e.g. remote session ended). */
  onSessionDisconnect?: (sessionId: string) => void;
  /** Lazy builder for a session-scoped `ConnectionService` client (session-participant routing). */
  buildSessionClient?: () => ConnectionClient | null;
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
  terminalControl,
  controlTokenRef,
  room = null,
  mobileShortcuts,
  onChildSessionStarted,
  traffic,
  runtimes = [],
  focusedRuntimeId = null,
  onSessionRoom,
  onSessionDisconnect,
  buildSessionClient,
}: SessionMainPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

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
  // background sessions keep streaming. The focused runtime additionally carries the
  // `sessions-detail-terminal-container` marker (existing acceptance contract) and the terminal
  // control mutex overlay.
  const hasRuntimes = runtimes.length > 0;

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
          {/* Inspector toggle button — always visible when a session is selected */}
          {selectedSession && (
            <div className="flex justify-end px-2 py-1 border-b border-border flex-shrink-0">
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
              {customView ? (
                // Custom per-workflow view — renders in place of the terminal regardless of
                // attachment status; the workflow owns its own chrome.
                customView
              ) : hasRuntimes ? (
                // One mounted terminal per attached session (focused visible, others hidden).
                <div
                  data-testid="sessions-runtime-layer"
                  className="flex-1 min-h-0 relative overflow-hidden"
                >
                  {runtimes.map((r) => {
                    const focused = r.sessionId === focusedRuntimeId;
                    return (
                      <div
                        key={r.sessionId}
                        data-testid={`sessions-runtime-terminal-${r.sessionId}`}
                        className={cn(
                          "absolute inset-0 h-full w-full",
                          focused ? "" : "hidden",
                        )}
                        aria-hidden={!focused}
                      >
                        {r.status === "connected-livekit" &&
                          tokenClient &&
                          r.livekitRoom && (
                            <div className="h-full w-full">
                              <SessionLiveKitTerminal
                                livekitUrl={r.livekitUrl ?? ""}
                                livekitRoom={r.livekitRoom}
                                livekitServerIdentity={r.livekitServerIdentity ?? ""}
                                identity={r.identity ?? ""}
                                tokenClient={tokenClient}
                                onDisconnect={() => onSessionDisconnect?.(r.sessionId)}
                                mobileShortcuts={focused ? mobileShortcuts : undefined}
                                onRoom={(sessionRoom) => onSessionRoom?.(r.sessionId, sessionRoom)}
                              />
                            </div>
                          )}
                        {r.status === "connected-livekit" && !tokenClient && (
                          <div className="h-full w-full text-xs text-muted-foreground p-4">
                            Terminal connected to {r.livekitRoom}
                          </div>
                        )}
                        {r.status === "connected-grpc" && client && (
                          <div className="h-full w-full">
                            <GrpcSessionTerminal
                              sessionId={r.sessionId}
                              sessionToken={sessionToken}
                              client={client}
                              controlToken={controlTokenRef?.current}
                              onDisconnect={() => onSessionDisconnect?.(r.sessionId)}
                              mobileShortcuts={focused ? mobileShortcuts : undefined}
                            />
                          </div>
                        )}
                        {focused && (
                          // The focused runtime carries the terminal-control mutex overlay and the
                          // `sessions-detail-terminal-container` marker (existing acceptance contract).
                          // `pointer-events-none` lets clicks reach the terminal below when no overlay
                          // is showing; the overlay itself re-enables pointer events.
                          <div
                            data-testid="sessions-detail-terminal-container"
                            className="absolute inset-0 pointer-events-none"
                          >
                            {terminalControl && !terminalControl.isController && (
                              <div
                                data-testid="terminal-control-overlay"
                                className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-background/80 backdrop-blur-sm pointer-events-auto"
                              >
                                <p className="text-sm text-muted-foreground mb-1">
                                  Controlled by another screen
                                </p>
                                <p
                                  data-testid="terminal-control-holder"
                                  className="text-xs text-muted-foreground mb-4 font-mono"
                                >
                                  {terminalControl.holderScreenId}
                                </p>
                                <Button
                                  data-testid="terminal-claim-btn"
                                  onClick={terminalControl.onClaim}
                                >
                                  Claim terminal
                                </Button>
                              </div>
                            )}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              ) : isConnected ? (
                // Connected but the runtime hasn't been registered yet (brief window before the
                // attach effect runs) — keep the terminal container marker so existing acceptance
                // contracts hold during the transition.
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
              )}
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
