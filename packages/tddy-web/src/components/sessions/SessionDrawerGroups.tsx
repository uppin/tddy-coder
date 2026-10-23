import React, { useState } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";
import {
  partitionSessionsByActivity,
  type SessionStackGroupResult,
} from "../../utils/sessionStackGroups";
import type { ProjectSessionGroup } from "../../utils/sessionProjectGroups";
import { projectGroupKey } from "./SessionProjectGroup";
import { Tooltip, TooltipTrigger, TooltipContent } from "../ui/tooltip";
import { SessionDrawerItem } from "./SessionDrawerItem";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import { SessionDrawerSeparator } from "./SessionDrawerSeparator";
import { sessionDrawerLabel } from "../../utils/sessionDrawerLabel";
import { cn } from "../../lib/utils";

// The rows the session drawer renders — stack groups, activity partitions, a project group's body
// and the collapsed strip's dots — split out of `SessionDrawer.tsx`, which keeps the drawer chrome
// and the project-home wiring.

/**
 * Owning-host attribution passed down so each row can badge cross-host sessions. A session's owning
 * daemon is its `daemonInstanceId`; `hostLabelForInstance` turns that id into a display label;
 * `selectedInstanceId` is the host currently selected (rows owned by it get no badge).
 */
export interface OwningHostInfo {
  selectedInstanceId: string;
  hostLabelForInstance: (instanceId: string) => string;
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

/**
 * Resolve the codebase-host badge label for a row. `codebaseDaemonInstanceId` is set only on a split
 * session (docs/ft/daemon/remote-managed-worktree.md), so it is badged regardless of which host is
 * selected: unlike the agent's host it cannot be inferred from the row's position in the list.
 */
function badgeCodebaseHostLabel(session: SessionEntry, info: OwningHostInfo): string | null {
  const codebaseHost = session.codebaseDaemonInstanceId.trim();
  if (!codebaseHost) return null;
  return info.hostLabelForInstance(codebaseHost);
}

function SessionStackGroup({
  group,
  selectedSessionId,
  onSelectSession,
  owningHost,
  sessionMetadataBySessionId,
  selectedForDelete,
  onToggleSelect,
}: {
  group: StackGroup;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  owningHost: OwningHostInfo;
  sessionMetadataBySessionId: ReadonlyMap<string, SessionMetadata>;
  selectedForDelete?: ReadonlySet<string>;
  onToggleSelect?: (sessionId: string) => void;
}) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <div data-testid={`sessions-drawer-stack-${group.parent.sessionId}`}>
      <SessionDrawerItem
        session={group.parent}
        isSelected={group.parent.sessionId === selectedSessionId}
        onClick={onSelectSession}
        hostLabel={badgeHostLabel(group.parent, owningHost)}
        codebaseHostLabel={badgeCodebaseHostLabel(group.parent, owningHost)}
        sessionMetadata={sessionMetadataBySessionId.get(group.parent.sessionId)}
        selected={selectedForDelete?.has(group.parent.sessionId)}
        onToggleSelect={onToggleSelect}
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
            codebaseHostLabel={badgeCodebaseHostLabel(child, owningHost)}
            sessionMetadata={sessionMetadataBySessionId.get(child.sessionId)}
            selected={selectedForDelete?.has(child.sessionId)}
            onToggleSelect={onToggleSelect}
          />
        ))}
      </div>
    </div>
  );
}

/**
 * One row of the collapsed (12px-wide) strip: the session's indicator dot and nothing else, with the
 * drawer label as its tooltip. It is a component rather than an inline `map` body because the
 * indicator is read through a per-session store subscription, which only a component can hold.
 */
export function CollapsedSessionDot({
  session,
  isSelected,
  onSelectSession,
}: {
  session: SessionEntry;
  isSelected: boolean;
  onSelectSession: (sessionId: string) => void;
}) {
  const label = sessionDrawerLabel(session);

  return (
    <Tooltip>
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
          <SessionIndicatorDot session={session} sizeClassName="h-2.5 w-2.5" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
}

/** Renders one partition's stack groups followed by its flat rows. */
function SessionPartitionBody({
  partition,
  selectedSessionId,
  onSelectSession,
  owningHost,
  sessionMetadataBySessionId,
  selectedForDelete,
  onToggleSelect,
}: {
  partition: SessionStackGroupResult;
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  owningHost: OwningHostInfo;
  sessionMetadataBySessionId: ReadonlyMap<string, SessionMetadata>;
  selectedForDelete?: ReadonlySet<string>;
  onToggleSelect?: (sessionId: string) => void;
}) {
  return (
    <>
      {partition.groups.map((group) => (
        <SessionStackGroup
          key={group.parent.sessionId}
          group={group}
          selectedSessionId={selectedSessionId}
          onSelectSession={onSelectSession}
          owningHost={owningHost}
          sessionMetadataBySessionId={sessionMetadataBySessionId}
          selectedForDelete={selectedForDelete}
          onToggleSelect={onToggleSelect}
        />
      ))}
      {partition.flat.map((session) => (
        <SessionDrawerItem
          key={session.sessionId}
          session={session}
          isSelected={session.sessionId === selectedSessionId}
          onClick={onSelectSession}
          hostLabel={badgeHostLabel(session, owningHost)}
          codebaseHostLabel={badgeCodebaseHostLabel(session, owningHost)}
          sessionMetadata={sessionMetadataBySessionId.get(session.sessionId)}
          selected={selectedForDelete?.has(session.sessionId)}
          onToggleSelect={onToggleSelect}
        />
      ))}
    </>
  );
}

/** One project group's body: today's Active/Remaining partition and stack nesting over its sessions. */
export function SessionGroupBody({
  sessions,
  selectionMode,
  partitionBodyProps,
}: {
  sessions: SessionEntry[];
  selectionMode: boolean;
  partitionBodyProps: Omit<React.ComponentProps<typeof SessionPartitionBody>, "partition">;
}) {
  const { active, remaining, activeCount, remainingCount } = partitionSessionsByActivity(sessions);
  const isPartitioned = activeCount > 0 && remainingCount > 0;

  if (!isPartitioned) {
    return (
      <SessionPartitionBody
        partition={activeCount > 0 ? active : remaining}
        {...partitionBodyProps}
      />
    );
  }
  return (
    <>
      <SessionDrawerSeparator
        testId="sessions-drawer-separator-active"
        label="Active"
        count={activeCount}
        defaultOpen
        forceOpen={selectionMode}
      >
        <SessionPartitionBody partition={active} {...partitionBodyProps} />
      </SessionDrawerSeparator>
      <SessionDrawerSeparator
        testId="sessions-drawer-separator-remaining"
        label="Remaining"
        count={remainingCount}
        defaultOpen={false}
        forceOpen={selectionMode}
      >
        <SessionPartitionBody partition={remaining} {...partitionBodyProps} />
      </SessionDrawerSeparator>
    </>
  );
}

/** The key of the project group that holds `sessionId`, or `null` when no group does. */
export function projectGroupKeyContaining(
  groups: ProjectSessionGroup[],
  sessionId: string | null,
): string | null {
  if (sessionId === null) return null;
  const group = groups.find((g) => g.sessions.some((s) => s.sessionId === sessionId));
  return group ? projectGroupKey(group) : null;
}
