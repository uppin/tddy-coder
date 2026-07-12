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
  /** Fired when the connected terminal disconnects — including automatically when it unmounts (session switch). */
  onDisconnect?: () => void;
  /** Shortcut presets for the connected session — shown as the mobile shortcut overlay. */
  mobileShortcuts?: ToolShortcutDef[];
  /** Fired when a custom workflow view (e.g. PrStackScreen) spawns a child session. */
  onChildSessionStarted?: (entry: {
    sessionId: string;
    recipe: string;
    orchestratorSessionId: string;
    projectId: string;
  }) => void;
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
  onDisconnect,
  mobileShortcuts,
  onChildSessionStarted,
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
              ) : isConnected ? (
                // Connected — show terminal container
                <div
                  data-testid="sessions-detail-terminal-container"
                  className="flex-1 min-h-0 flex flex-col relative overflow-hidden"
                >
                  {attachment.status === "connected-livekit" && tokenClient && (
                    <div className="flex-1 min-h-0" style={{ minWidth: 0 }}>
                      <SessionLiveKitTerminal
                        livekitUrl={attachment.livekitUrl}
                        livekitRoom={attachment.livekitRoom}
                        livekitServerIdentity={attachment.livekitServerIdentity}
                        identity={attachment.identity}
                        tokenClient={tokenClient}
                        onDisconnect={onDisconnect}
                        mobileShortcuts={mobileShortcuts}
                      />
                    </div>
                  )}
                  {attachment.status === "connected-livekit" && !tokenClient && (
                    <div className="flex-1 min-h-0 text-xs text-muted-foreground p-4">
                      Terminal connected to {attachment.livekitRoom}
                    </div>
                  )}
                  {attachment.status === "connected-grpc" && client && (
                    <div className="flex-1 min-h-0" style={{ minWidth: 0 }}>
                      <GrpcSessionTerminal
                        sessionId={attachment.sessionId}
                        sessionToken={sessionToken}
                        client={client}
                        controlToken={controlTokenRef?.current}
                        onDisconnect={onDisconnect}
                        mobileShortcuts={mobileShortcuts}
                      />
                    </div>
                  )}
                  {/* Terminal control mutex overlay */}
                  {terminalControl && !terminalControl.isController && (
                    <div
                      data-testid="terminal-control-overlay"
                      className="absolute inset-0 z-10 flex flex-col items-center justify-center bg-background/80 backdrop-blur-sm"
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
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
