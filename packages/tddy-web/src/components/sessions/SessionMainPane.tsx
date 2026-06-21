import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import type { InspectorDrawerState } from "./SessionInspectorDrawer";
import { SessionInspectorDrawer } from "./SessionInspectorDrawer";
import { Button } from "../ui/button";

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
}: SessionMainPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  return (
    <div
      data-testid="sessions-detail-pane"
      className="flex-1 min-w-0 flex flex-col h-full overflow-hidden relative"
    >
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
              {/* Terminal mounts here; in CT the LiveKit connection may not fully initialize */}
              Terminal connected to {attachment.livekitRoom}
            </div>
          )}
          {attachment.status === "connected-grpc" && (
            <div className="flex-1 min-h-0 text-xs text-muted-foreground p-4">
              gRPC terminal connected
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
          />
        </div>
      )}
    </div>
  );
}
