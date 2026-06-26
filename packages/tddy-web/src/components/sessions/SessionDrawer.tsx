import React from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { SessionEntry } from "../../gen/connection_pb";
import { ScrollArea } from "../ui/scroll-area";
import { Tooltip, TooltipTrigger, TooltipContent } from "../ui/tooltip";
import { SessionDrawerItem } from "./SessionDrawerItem";
import { sessionDrawerLabel } from "../../utils/sessionDrawerLabel";
import { connectionStatusForSession } from "../../utils/connectionStatusForSession";
import { cn } from "../../lib/utils";

const STATUS_COLOR: Record<string, string> = {
  connected: "bg-green-500",
  disconnected: "bg-gray-400",
  "needs-input": "bg-yellow-500",
};

interface SessionDrawerProps {
  sessions: SessionEntry[];
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  onCreateSession?: () => void;
  isOpen: boolean;
  onClose: () => void;
  onOpen: () => void;
}

export function SessionDrawer({
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  isOpen,
  onClose,
  onOpen,
}: SessionDrawerProps) {
  if (!isOpen) {
    // Strip mode on desktop; hidden on mobile
    return (
      <div
        data-testid="sessions-drawer"
        data-drawer-state="closed"
        className="hidden md:flex flex-col h-full border-r border-border bg-background flex-shrink-0 w-12"
      >
        {/* Open button */}
        <div className="border-b border-border flex items-center justify-center py-2">
          <button
            type="button"
            data-testid="sessions-drawer-open-btn"
            onClick={onOpen}
            className="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
            title="Open session list"
          >
            <ChevronRight className="h-4 w-4" />
          </button>
        </div>

        {/* Status dots — one per session, clickable */}
        <div className="py-2 flex flex-col items-center gap-2 overflow-y-auto">
          {sessions.map((session) => {
            const status = connectionStatusForSession(session);
            const label = sessionDrawerLabel(session);
            const isSelected = session.sessionId === selectedSessionId;
            return (
              <Tooltip key={session.sessionId}>
                <TooltipTrigger asChild>
                  <button
                    type="button"
                    onClick={() => onSelectSession(session.sessionId)}
                    className={cn(
                      "h-8 w-8 flex items-center justify-center rounded-md transition-colors",
                      isSelected ? "bg-accent" : "hover:bg-muted",
                    )}
                    title={label}
                  >
                    <span
                      className={cn(
                        "h-2.5 w-2.5 rounded-full flex-shrink-0",
                        STATUS_COLOR[status] ?? "bg-gray-400",
                      )}
                    />
                  </button>
                </TooltipTrigger>
                <TooltipContent side="right">{label}</TooltipContent>
              </Tooltip>
            );
          })}
        </div>
      </div>
    );
  }

  return (
    <div
      data-testid="sessions-drawer"
      data-drawer-state="open"
      className="flex flex-col h-full border-r border-border bg-background flex-shrink-0 w-[280px]"
    >
      <div className="px-3 py-2 border-b border-border flex items-center gap-1">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground flex-1">
          Sessions
        </span>
        {onCreateSession && (
          <button
            type="button"
            data-testid="sessions-drawer-new-btn"
            onClick={onCreateSession}
            className="text-xs text-muted-foreground hover:text-foreground transition-colors"
            title="New session"
          >
            + New session
          </button>
        )}
        <button
          type="button"
          data-testid="sessions-drawer-close-btn"
          onClick={onClose}
          className="p-1 rounded hover:bg-muted transition-colors text-muted-foreground hover:text-foreground"
          title="Close session list"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
      </div>
      <ScrollArea className="flex-1 min-h-0">
        <div className="py-1 px-2 space-y-0.5">
          {sessions.map((session) => (
            <SessionDrawerItem
              key={session.sessionId}
              session={session}
              isSelected={session.sessionId === selectedSessionId}
              onClick={onSelectSession}
            />
          ))}
          {sessions.length === 0 && (
            <div className="px-3 py-4 text-sm text-muted-foreground text-center">
              No sessions
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
