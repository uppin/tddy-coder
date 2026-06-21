import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import type { SessionAttachmentState } from "./useSessionAttachment";
import { Button } from "../ui/button";

interface SessionDetailPaneProps {
  selectedSession: SessionEntry | null;
  attachment: SessionAttachmentState;
  onConnect: (sessionId: string) => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
}

export function SessionDetailPane({
  selectedSession,
  attachment,
  onConnect: _onConnect,
  onResume,
  onDelete,
}: SessionDetailPaneProps) {
  const isConnected =
    attachment.status === "connected-livekit" || attachment.status === "connected-grpc";

  return (
    <div
      data-testid="sessions-detail-pane"
      className="flex-1 min-w-0 flex flex-col h-full overflow-hidden"
    >
      {!selectedSession ? (
        // No session selected
        <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
          Select a session
        </div>
      ) : isConnected ? (
        // Connected — show terminal container
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
        </div>
      ) : (
        // Disconnected / idle — show metadata + controls
        <div className="flex flex-col gap-4 p-4">
          <div
            data-testid="sessions-detail-metadata"
            className="flex flex-col gap-1 text-sm"
          >
            {selectedSession.workflowGoal && (
              <div>
                <span className="font-medium">Goal:</span> {selectedSession.workflowGoal}
              </div>
            )}
            <div>
              <span className="font-medium">Status:</span> {selectedSession.status}
            </div>
            {selectedSession.repoPath && (
              <div>
                <span className="font-medium">Repo:</span> {selectedSession.repoPath}
              </div>
            )}
            {selectedSession.createdAt && (
              <div>
                <span className="font-medium">Created:</span> {selectedSession.createdAt}
              </div>
            )}
            {selectedSession.elapsedDisplay && (
              <div>
                <span className="font-medium">Elapsed:</span> {selectedSession.elapsedDisplay}
              </div>
            )}
            {selectedSession.agent && (
              <div>
                <span className="font-medium">Agent:</span> {selectedSession.agent}
              </div>
            )}
            {selectedSession.model && (
              <div>
                <span className="font-medium">Model:</span> {selectedSession.model}
              </div>
            )}
          </div>

          <div className="flex gap-2">
            <Button
              data-testid={`sessions-detail-resume-${selectedSession.sessionId}`}
              onClick={() => onResume(selectedSession.sessionId)}
              size="sm"
            >
              Resume
            </Button>
            <Button
              data-testid={`sessions-detail-delete-${selectedSession.sessionId}`}
              onClick={() => onDelete(selectedSession.sessionId)}
              variant="destructive"
              size="sm"
              disabled
            >
              Delete
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
