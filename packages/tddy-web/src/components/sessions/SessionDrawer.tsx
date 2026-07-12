import React, { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { SessionEntry } from "../../gen/connection_pb";
import { groupSessionsByStack } from "../../utils/sessionStackGroups";
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

/**
 * Owning-host attribution passed down so each row can badge cross-host sessions. A session's owning
 * daemon is its `daemonInstanceId`; `hostLabelForInstance` turns that id into a display label;
 * `selectedInstanceId` is the host currently selected (rows owned by it get no badge).
 */
interface OwningHostInfo {
  selectedInstanceId: string;
  hostLabelForInstance: (instanceId: string) => string;
}

interface SessionDrawerProps {
  sessions: SessionEntry[];
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  onCreateSession?: () => void;
  isOpen: boolean;
  onClose: () => void;
  onOpen: () => void;
  /** On mobile the open list is a full-width overlay (out of flow) so it doesn't resize the terminal. */
  isMobile?: boolean;
  /** The currently selected host — rows owned by it get no badge. Defaults to `""`. */
  selectedInstanceId?: string;
  /** Turn a daemon instance id into its display label. Defaults to the identity function. */
  hostLabelForInstance?: (instanceId: string) => string;
}

interface StackGroup {
  parent: SessionEntry;
  children: SessionEntry[];
}

/** Resolve the owning-host badge label for a row, or `null` when it belongs to the selected host. */
function badgeHostLabel(session: SessionEntry, info: OwningHostInfo): string | null {
  const owner = session.daemonInstanceId.trim() || info.selectedInstanceId;
  if (owner === info.selectedInstanceId) return null;
  return info.hostLabelForInstance(owner);
}

function SessionStackGroup({
  group,
  selectedSessionId,
  onSelectSession,
  owningHost,
}: {
  group: StackGroup;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  owningHost: OwningHostInfo;
}) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <div data-testid={`sessions-drawer-stack-${group.parent.sessionId}`}>
      <SessionDrawerItem
        session={group.parent}
        isSelected={group.parent.sessionId === selectedSessionId}
        onClick={onSelectSession}
        hostLabel={badgeHostLabel(group.parent, owningHost)}
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
            hostLabel={badgeHostLabel(child, owningHost)}
          />
        ))}
      </div>
    </div>
  );
}

export function SessionDrawer({
  sessions,
  selectedSessionId,
  onSelectSession,
  onCreateSession,
  isOpen,
  onClose,
  onOpen,
  isMobile = false,
  selectedInstanceId = "",
  hostLabelForInstance = (instanceId) => instanceId,
}: SessionDrawerProps) {
  const owningHost: OwningHostInfo = {
    selectedInstanceId,
    hostLabelForInstance,
  };
  if (!isOpen) {
    // Strip mode on desktop; hidden on mobile (mobile uses the floating overlay
    // open button rendered by SessionsDrawerScreen).
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

  const { groups, flat } = groupSessionsByStack(sessions);

  // On mobile, the open list is a full-width overlay (absolute, out of flow) so it
  // sits on top of the terminal instead of resizing it. On desktop it's an in-flow
  // 280px column.
  return (
    <div
      data-testid="sessions-drawer"
      data-drawer-state="open"
      className={cn(
        "flex flex-col h-full border-r border-border bg-background",
        isMobile ? "w-full" : "flex-shrink-0 w-[280px]",
      )}
      style={isMobile ? { position: "absolute", inset: 0, width: "100%", zIndex: 30 } : undefined}
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
          {groups.map((group) => (
            <SessionStackGroup
              key={group.parent.sessionId}
              group={group}
              selectedSessionId={selectedSessionId}
              onSelectSession={onSelectSession}
              owningHost={owningHost}
            />
          ))}
          {flat.map((session) => (
            <SessionDrawerItem
              key={session.sessionId}
              session={session}
              isSelected={session.sessionId === selectedSessionId}
              onClick={onSelectSession}
              hostLabel={badgeHostLabel(session, owningHost)}
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
