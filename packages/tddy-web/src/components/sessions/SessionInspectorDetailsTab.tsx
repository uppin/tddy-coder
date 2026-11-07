import type { SessionEntry } from "../../gen/session_pb";
import { Button, ScrollArea } from "tddy-ui-web";
import { formatLastDataReceived } from "./lastDataReceivedFormat";
import { formatBytes } from "./formatTraffic";

/** Inspector I/O traffic: byte counters and the last time data arrived. */
export type InspectorTraffic = {
  bytesIn: number;
  bytesOut: number;
  lastDataReceivedAt: number | null;
};

export interface SessionInspectorDetailsTabProps {
  session: SessionEntry;
  traffic: InspectorTraffic | null;
  /**
   * Whether Delete is waiting for its confirmation. Held by the drawer, so switching tabs and back
   * keeps a half-confirmed delete where the operator left it.
   */
  pendingDelete: boolean;
  onPendingDeleteChange: (pending: boolean) => void;
  onResume: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onTerminate: (sessionId: string) => void;
}

function MetaRow({ label, value }: { label: string; value: string | number | undefined | null }) {
  if (value === undefined || value === null || value === "" || value === 0) return null;
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-xs break-all">{String(value)}</span>
    </div>
  );
}

/** The inspector's Details tab: the session's metadata, its I/O traffic, and its lifecycle controls. */
export function SessionInspectorDetailsTab({
  session,
  traffic,
  pendingDelete,
  onPendingDeleteChange,
  onResume,
  onDelete,
  onTerminate,
}: SessionInspectorDetailsTabProps) {
  return (
    <ScrollArea className="flex-1 min-h-0">
      <div className="px-3 py-3 flex flex-col gap-4">
        {/* Metadata */}
        <div data-testid="sessions-inspector-metadata" className="flex flex-col gap-2">
          <MetaRow label="Goal" value={session.workflowGoal} />
          <MetaRow label="Status" value={session.status} />
          <MetaRow label="Repo" value={session.repoPath} />
          <MetaRow label="Session ID" value={session.sessionId} />
          <MetaRow label="PID" value={session.pid} />
          <MetaRow label="Workflow state" value={session.workflowState} />
          <MetaRow label="Activity status" value={session.activityStatus} />
          <MetaRow label="Agent" value={session.agent} />
          <MetaRow label="Model" value={session.model} />
          <MetaRow label="Created" value={session.createdAt} />
          <MetaRow label="Updated" value={session.updatedAt} />
          <MetaRow label="Elapsed" value={session.elapsedDisplay} />
          <MetaRow label="Tool" value={session.tool} />
          <MetaRow label="Session type" value={session.sessionType} />
          <MetaRow label="LiveKit room" value={session.livekitRoom} />
          <MetaRow label="Previous session" value={session.previousSessionId} />
        </div>

        {/* I/O traffic (req 5 dual source) — bytes in / bytes out / last data received. */}
        <div className="flex flex-col gap-1">
          <span data-testid="sessions-inspector-bytes-in" className="text-xs">
            {formatBytes(traffic?.bytesIn ?? 0)}
          </span>
          <span data-testid="sessions-inspector-bytes-out" className="text-xs">
            {formatBytes(traffic?.bytesOut ?? 0)}
          </span>
          <span data-testid="sessions-inspector-last-data-received" className="text-xs">
            {formatLastDataReceived(traffic?.lastDataReceivedAt ?? null, Date.now())}
          </span>
        </div>

        {/* Controls */}
        <div className="flex flex-col gap-2">
          {!session.isActive && (
            <Button
              data-testid={`sessions-inspector-resume-${session.sessionId}`}
              size="sm"
              onClick={() => {
                onPendingDeleteChange(false);
                onResume(session.sessionId);
              }}
            >
              Resume
            </Button>
          )}

          {!session.isActive && !pendingDelete && (
            <Button
              data-testid={`sessions-inspector-delete-${session.sessionId}`}
              variant="destructive"
              size="sm"
              onClick={() => onPendingDeleteChange(true)}
            >
              Delete
            </Button>
          )}

          {!session.isActive && pendingDelete && (
            <Button
              data-testid={`sessions-inspector-delete-confirm-${session.sessionId}`}
              variant="destructive"
              size="sm"
              onClick={() => {
                onPendingDeleteChange(false);
                onDelete(session.sessionId);
              }}
            >
              Confirm delete
            </Button>
          )}

          {session.isActive && (
            <Button
              data-testid={`sessions-inspector-terminate-${session.sessionId}`}
              variant="destructive"
              size="sm"
              onClick={() => onTerminate(session.sessionId)}
            >
              Terminate
            </Button>
          )}
        </div>
      </div>
    </ScrollArea>
  );
}
