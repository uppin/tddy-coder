import React, { useState, useEffect, type MutableRefObject } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService, SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { SessionTrafficStrip } from "./SessionTrafficStrip";
import { useSessionLiveKitRoom } from "./useSessionLiveKitRoom";
import { useLiveKitPing } from "../../rpc/livekitPing";
import { useTrafficMeterRegistry } from "../../rpc/transportProvider";
import type { TrafficMeter } from "../../rpc/trafficMeter";
import { Button } from "../ui/button";
import { CreateSessionPane } from "./CreateSessionPane";
import { GrpcSessionTerminal } from "./GrpcSessionTerminal";
import type { TerminalControlState } from "./terminalControlState";

// ---------------------------------------------------------------------------
// Local hook — subscribe to a TrafficMeter and return a live snapshot.
// ---------------------------------------------------------------------------

type MeterSnap = { bytesIn: number; bytesOut: number; inRate: number; outRate: number };
const ZERO_SNAP: MeterSnap = { bytesIn: 0, bytesOut: 0, inRate: 0, outRate: 0 };

function useMeterSnapshot(meter: TrafficMeter | null): MeterSnap {
  const [snap, setSnap] = useState<MeterSnap>(() => (meter ? meter.snapshot() : ZERO_SNAP));
  useEffect(() => {
    if (!meter) {
      setSnap(ZERO_SNAP);
      return;
    }
    setSnap(meter.snapshot());
    return meter.subscribe(() => setSnap(meter.snapshot()));
  }, [meter]);
  return snap;
}

type ConnectionClient = Client<typeof ConnectionService>;

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
  sessionToken?: string;
  onCancelCreate?: () => void;
  onSessionCreated?: (sessionId: string) => void;
  // Terminal control state — when present and not the controller, renders a "Claim terminal" CTA.
  terminalControl?: TerminalControlState & { onClaim: () => void };
  /** Ref to the live control token from useTerminalControl. Passed through to GrpcSessionTerminal. */
  controlTokenRef?: MutableRefObject<string>;
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
  sessionToken = "",
  onCancelCreate,
  onSessionCreated,
  terminalControl,
  controlTokenRef,
}: SessionMainPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  // Traffic strip data — live meter snapshots + WebRTC ping.
  const livekitRoomName =
    attachment.status === "connected-livekit" ? attachment.livekitRoom : null;
  const { room } = useSessionLiveKitRoom(attachment);
  const pingMs = useLiveKitPing(room);
  const meterRegistry = useTrafficMeterRegistry();
  const httpSnap = useMeterSnapshot(meterRegistry?.get("http") ?? null);
  const livekitSnap = useMeterSnapshot(
    livekitRoomName && meterRegistry ? meterRegistry.get(livekitRoomName) : null,
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
          {/* Traffic strip — only visible when connected via LiveKit */}
          {selectedSession && attachment.status === "connected-livekit" && (
            <SessionTrafficStrip
              bytesIn={httpSnap.bytesIn + livekitSnap.bytesIn}
              bytesOut={httpSnap.bytesOut + livekitSnap.bytesOut}
              inRate={httpSnap.inRate + livekitSnap.inRate}
              outRate={httpSnap.outRate + livekitSnap.outRate}
              pingMs={pingMs}
            />
          )}

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
          ) : isConnected ? (
            // Connected — show terminal container (with inspector overlay)
            <div
              data-testid="sessions-detail-terminal-container"
              className="flex-1 min-h-0 flex flex-col relative overflow-hidden"
            >
              {attachment.status === "connected-livekit" && (
                <div className="flex-1 min-h-0 text-xs text-muted-foreground p-4">
                  {/* TODO: render GhosttyTerminalLiveKit here (needs token from TokenService) */}
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
              {/* Inspector overlay */}
              <SessionInspectorDrawer
                key={selectedSession.sessionId}
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
              />
            </div>
          ) : (
            // Disconnected / idle — simple placeholder with inspector as overlay
            <div className="flex-1 min-h-0 relative overflow-hidden">
              <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
                Select Resume to reconnect
              </div>
              {/* Inspector overlay */}
              <SessionInspectorDrawer
                key={selectedSession.sessionId}
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
              />
            </div>
          )}
        </>
      )}
    </div>
  );
}
