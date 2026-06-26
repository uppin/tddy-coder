import React, { useState } from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { groupSessionsByStack } from "../../utils/sessionStackGroups";
import { ScrollArea } from "../ui/scroll-area";
import { SessionDrawerItem } from "./SessionDrawerItem";

interface SessionDrawerProps {
  sessions: SessionEntry[];
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  onCreateSession?: () => void;
}

interface StackGroup {
  parent: SessionEntry;
  children: SessionEntry[];
}

function SessionStackGroup({
  group,
  selectedSessionId,
  onSelectSession,
}: {
  group: StackGroup;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
}) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <div data-testid={`sessions-drawer-stack-${group.parent.sessionId}`}>
      <SessionDrawerItem
        session={group.parent}
        isSelected={group.parent.sessionId === selectedSessionId}
        onClick={onSelectSession}
      />
      {/* <details> provides the <summary> toggle target; children visibility is controlled explicitly via React state */}
      <details>
        <summary
          className="list-none cursor-pointer py-0.5"
          onClick={(e) => {
            e.preventDefault();
            setIsOpen((v) => !v);
          }}
        />
      </details>
      <div style={isOpen ? undefined : { display: "none" }}>
        {group.children.map((child) => (
          <SessionDrawerItem
            key={child.sessionId}
            session={child}
            isSelected={child.sessionId === selectedSessionId}
            onClick={onSelectSession}
            depth={1}
          />
        ))}
      </div>
    </div>
  );
}

export function SessionDrawer({ sessions, selectedSessionId, onSelectSession, onCreateSession }: SessionDrawerProps) {
  const { groups, flat } = groupSessionsByStack(sessions);

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
          {groups.map((group) => (
            <SessionStackGroup
              key={group.parent.sessionId}
              group={group}
              selectedSessionId={selectedSessionId}
              onSelectSession={onSelectSession}
            />
          ))}
          {flat.map((session) => (
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
