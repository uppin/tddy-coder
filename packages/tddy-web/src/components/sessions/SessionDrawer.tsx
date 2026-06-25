import React from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { ScrollArea } from "../ui/scroll-area";
import { SessionDrawerItem } from "./SessionDrawerItem";

interface SessionDrawerProps {
  sessions: SessionEntry[];
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  onCreateSession?: () => void;
}

export function SessionDrawer({ sessions, selectedSessionId, onSelectSession, onCreateSession }: SessionDrawerProps) {
  return (
    <div
      data-testid="sessions-drawer"
      className="flex flex-col h-full border-r border-border bg-background"
      style={{ width: 280, flexShrink: 0 }}
    >
      <div className="px-3 py-2 border-b border-border flex items-center justify-between">
        <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
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
