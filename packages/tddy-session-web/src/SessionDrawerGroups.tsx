import React, { useState } from "react";
import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import type { SessionMetadata } from "./lib/sessionParticipantMetadata";
import { partitionSessionsByActivity, type SessionStackGroupResult } from "./utils/sessionStackGroups";
import type { SubagentSessionNode } from "./agentTree";
import type { ProjectSessionGroup } from "./utils/sessionProjectGroups";
import { projectGroupKey } from "./SessionProjectGroup";
import { Tooltip, TooltipTrigger, TooltipContent, cn } from "tddy-ui-web";
import { SessionDrawerItem } from "./SessionDrawerItem";
import { SessionIndicatorDot } from "./SessionIndicatorDot";
import { SessionDrawerSeparator } from "./SessionDrawerSeparator";
import { sessionDrawerLabel } from "./utils/sessionDrawerLabel";

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

/** The props every drawer row shares, whichever session it renders. */
interface DrawerRowProps {
  selectedSessionId: string | null;
  onSelectSession: (sessionId: string) => void;
  owningHost: OwningHostInfo;
  sessionMetadataBySessionId: ReadonlyMap<string, SessionMetadata>;
  selectedForDelete?: ReadonlySet<string>;
  onToggleSelect?: (sessionId: string) => void;
}

function DrawerRow({ session, depth, ...props }: DrawerRowProps & { session: SessionEntry; depth?: number }) {
  return (
    <SessionDrawerItem
      session={session}
      isSelected={session.sessionId === props.selectedSessionId}
      onClick={props.onSelectSession}
      depth={depth}
      hostLabel={badgeHostLabel(session, props.owningHost)}
      codebaseHostLabel={badgeCodebaseHostLabel(session, props.owningHost)}
      sessionMetadata={props.sessionMetadataBySessionId.get(session.sessionId)}
      selected={props.selectedForDelete?.has(session.sessionId)}
      onToggleSelect={props.onToggleSelect}
    />
  );
}

/** The sessions `nodes` started, depth-first, so each lands right after its starter. */
function descendantRows(
  nodes: ReadonlyArray<SubagentSessionNode>,
  depth: number,
): Array<{ session: SessionEntry; depth: number }> {
  return nodes.flatMap((node) => [{ session: node.session, depth }, ...descendantRows(node.children, depth + 1)]);
}

/** A starter's row with every session beneath it, behind one toggle that folds them. */
function SessionStackGroup({ node, ...rowProps }: DrawerRowProps & { node: SubagentSessionNode }) {
  const [isOpen, setIsOpen] = useState(true);

  return (
    <div data-testid={`sessions-drawer-stack-${node.session.sessionId}`}>
      <DrawerRow session={node.session} {...rowProps} />
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
        {descendantRows(node.children, 1).map(({ session, depth }) => (
          <DrawerRow key={session.sessionId} session={session} depth={depth} {...rowProps} />
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

/** Renders one partition's rows in order: a starter with its started sessions as a group, the rest bare. */
function SessionPartitionBody({ partition, ...rowProps }: DrawerRowProps & { partition: SessionStackGroupResult }) {
  return (
    <>
      {partition.rows.map((node) =>
        node.children.length > 0 ? (
          <SessionStackGroup key={node.session.sessionId} node={node} {...rowProps} />
        ) : (
          <DrawerRow key={node.session.sessionId} session={node.session} {...rowProps} />
        ),
      )}
    </>
  );
}

/**
 * One project group's body: its active sessions newest-started first, then the rest under a
 * collapsed **More**, most recently touched first — each stack-nested, a started session right
 * after its starter.
 */
export function SessionGroupBody({
  sessions,
  selectionMode,
  partitionBodyProps,
}: {
  sessions: SessionEntry[];
  selectionMode: boolean;
  partitionBodyProps: Omit<React.ComponentProps<typeof SessionPartitionBody>, "partition">;
}) {
  const { active, remaining, remainingCount } = partitionSessionsByActivity(sessions);

  // Active sessions are listed bare; the rest always fold under a collapsed "More" — also when
  // nothing is active, so an idle project shows only its pinned main and the fold.
  return (
    <>
      <SessionPartitionBody partition={active} {...partitionBodyProps} />
      {remainingCount > 0 && (
        <SessionDrawerSeparator
          testId="sessions-drawer-separator-remaining"
          label="More"
          count={remainingCount}
          defaultOpen={false}
          forceOpen={selectionMode}
        >
          <SessionPartitionBody partition={remaining} {...partitionBodyProps} />
        </SessionDrawerSeparator>
      )}
    </>
  );
}

/**
 * Projects with at least one active session first, then the rest — a stable partition, so each side
 * keeps the order it came in. The trailing **Other** group is not a project and stays last.
 */
export function pinProjectsWithActiveSessions(groups: ProjectSessionGroup[]): ProjectSessionGroup[] {
  const hasActive = (g: ProjectSessionGroup) => partitionSessionsByActivity(g.sessions).activeCount > 0;
  const projectGroups = groups.filter((g) => g.projectId !== null);
  return [
    ...projectGroups.filter(hasActive),
    ...projectGroups.filter((g) => !hasActive(g)),
    ...groups.filter((g) => g.projectId === null),
  ];
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
