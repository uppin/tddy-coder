import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { sessionDrawerLabel } from "../../utils/sessionDrawerLabel";
import { connectionStatusForSession } from "../../utils/connectionStatusForSession";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";
import { Tooltip, TooltipTrigger, TooltipContent } from "../ui/tooltip";
import { cn } from "../../lib/utils";

interface SessionDrawerItemProps {
  session: SessionEntry;
  isSelected: boolean;
  onClick: (sessionId: string) => void;
  depth?: number;
  /** Owning-host label for a cross-host row; `null`/omitted for rows on the selected host. */
  hostLabel?: string | null;
  /** Parsed `session` participant-metadata block for this row (presence-driven, req 4). */
  sessionMetadata?: SessionMetadata | null;
}

const STATUS_COLOR: Record<string, string> = {
  connected: "bg-green-500",
  disconnected: "bg-gray-400",
  "needs-input": "bg-yellow-500",
};

export function SessionDrawerItem({ session, isSelected, onClick, depth, hostLabel, sessionMetadata }: SessionDrawerItemProps) {
  const label = sessionDrawerLabel(session);
  const status = connectionStatusForSession(session);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-testid={`sessions-drawer-item-${session.sessionId}`}
          data-depth={depth !== undefined ? String(depth) : undefined}
          aria-selected={isSelected ? "true" : undefined}
          onClick={() => onClick(session.sessionId)}
          className={cn(
            "w-full text-left flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors",
            isSelected
              ? "bg-accent text-accent-foreground"
              : "hover:bg-muted",
          )}
        >
          {/* Status dot */}
          <span
            data-testid={`sessions-drawer-item-status-${session.sessionId}`}
            data-status={status}
            className={cn("flex-shrink-0 h-2 w-2 rounded-full", STATUS_COLOR[status])}
          />
          {/* Label */}
          <span
            data-testid={`sessions-drawer-item-label-${session.sessionId}`}
            className="truncate flex-1"
          >
            {label}
          </span>
          {/* Owning-host badge — only for cross-host rows (a session owned by a non-selected host). */}
          {hostLabel && (
            <span
              data-testid={`sessions-drawer-item-host-${session.sessionId}`}
              className="flex-shrink-0 text-[10px] leading-none px-1.5 py-0.5 rounded bg-muted text-muted-foreground"
            >
              {hostLabel}
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent
        data-testid={`sessions-drawer-item-tooltip-${session.sessionId}`}
        side="right"
      >
        {session.sessionId}
      </TooltipContent>
      {/* Parsed `session` participant-metadata block (req 4) — presence-driven from LiveKit. */}
      {sessionMetadata && (
        <div
          data-testid={`sessions-drawer-item-session-meta-${session.sessionId}`}
          className="px-3 pb-1 -mt-1 text-[10px] leading-tight text-muted-foreground"
        >
          <span className="truncate block">{sessionMetadata.workflowGoal}</span>
          <span className="flex items-center gap-1">
            <span>{sessionMetadata.workflowState}</span>
            <span>·</span>
            <span>{sessionMetadata.agent}</span>
            <span>·</span>
            <span>{sessionMetadata.model}</span>
          </span>
        </div>
      )}
    </Tooltip>
  );
}
