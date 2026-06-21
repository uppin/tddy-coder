import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { sessionDrawerLabel } from "../../utils/sessionDrawerLabel";
import { connectionStatusForSession } from "../../utils/connectionStatusForSession";
import { Tooltip, TooltipTrigger, TooltipContent } from "../ui/tooltip";
import { cn } from "../../lib/utils";

interface SessionDrawerItemProps {
  session: SessionEntry;
  isSelected: boolean;
  onClick: (sessionId: string) => void;
}

const STATUS_COLOR: Record<string, string> = {
  connected: "bg-green-500",
  disconnected: "bg-gray-400",
  "needs-input": "bg-yellow-500",
};

export function SessionDrawerItem({ session, isSelected, onClick }: SessionDrawerItemProps) {
  const label = sessionDrawerLabel(session);
  const status = connectionStatusForSession(session);

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-testid={`sessions-drawer-item-${session.sessionId}`}
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
        </button>
      </TooltipTrigger>
      <TooltipContent
        data-testid={`sessions-drawer-item-tooltip-${session.sessionId}`}
        side="right"
      >
        {session.sessionId}
      </TooltipContent>
    </Tooltip>
  );
}
