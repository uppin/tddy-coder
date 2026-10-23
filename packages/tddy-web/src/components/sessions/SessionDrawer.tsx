import React, { useEffect, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { SessionEntry } from "../../gen/session_pb";
import type { SessionMetadata } from "../../lib/sessionParticipantMetadata";
import { groupSessionsByProject } from "../../utils/sessionProjectGroups";
import {
  projectHomeSlot,
  withProjectHomeGroups,
  type ProjectHomeInfo,
} from "../../utils/projectHomeGroups";
import { SessionProjectGroup, projectGroupKey } from "./SessionProjectGroup";
import { ProjectHomeEntry } from "./ProjectHomeEntry";
import { readCollapsedProjects, writeCollapsedProjects } from "./projectGroupCollapse";
import { ScrollArea } from "../ui/scroll-area";
import { SESSION_INDICATOR_DOT_STYLES } from "./sessionIndicatorDotStyles";
import { cn } from "../../lib/utils";
import {
  CollapsedSessionDot,
  SessionGroupBody,
  projectGroupKeyContaining,
  type OwningHostInfo,
} from "./SessionDrawerGroups";


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
  /**
   * Whether the connection in scope can see sessions on hosts other than the selected one — the
   * answer `SessionsDrawerScreen` gets from `useHasCapability(hostConnection, "presence")`.
   *
   * A cross-host row is a live LiveKit coder participant, so on a wire with no presence the union
   * quietly narrows to what `ListSessions` reports for the selected host. Required, and required to
   * be answered rather than defaulted, because the failure it guards against is silence: a list that
   * has lost rows looks exactly like a list that never had them. When it is `false` the drawer shows
   * the rows it does have and says which ones it cannot see (PRD AC 5).
   */
  crossHostSessionsVisible: boolean;
  /** Parsed `session` participant-metadata, keyed by session id (req 4). Defaults to empty. */
  sessionMetadataBySessionId?: ReadonlyMap<string, SessionMetadata>;
  /** Session ids ticked for bulk delete. When provided, each row renders a selection checkbox. */
  selectedForDelete?: ReadonlySet<string>;
  /** Toggle a row's bulk-delete selection. Required (with selection mode on) for row checkboxes. */
  onToggleSelect?: (sessionId: string) => void;
  /** Whether bulk-selection mode is active — drives the bottom minibar layout. */
  selectionMode?: boolean;
  /** Activate selection mode from the bottom minibar (reveals per-row checkboxes). */
  onEnterSelectionMode?: () => void;
  /** Exit selection mode and clear the current selection. */
  onExitSelectionMode?: () => void;
  /** Toggle select-all / select-none across the visible sessions. */
  onSelectAll?: () => void;
  /** Delete the currently selected sessions. */
  onBulkDelete?: () => void;
  /**
   * Failures about the list itself — it could not be loaded, a bulk delete left rows behind —
   * shown under the header, above the rows they are about. Rendered only while the list is open.
   */
  notices?: React.ReactNode;
  /**
   * Projects the drawer knows by name — the headers of its per-project groups (`#project-home`
   * 2/7). A session whose project is not here lands in the trailing **Other** group.
   */
  projects?: ReadonlyArray<{ projectId: string; name: string }>;
  /**
   * What each project group's pinned **main** entry shows (`#project-home` 4/7): the project's main
   * branch ref and its home agent's label (`null` when no home agent is set — the entry then shows
   * a hint instead of starting anything). A project absent here gets no pinned entry.
   */
  projectHomes?: ReadonlyArray<ProjectHomeInfo>;
  /** Start, resume or reach the project's home session. Rejects with the daemon's reason. */
  onEnsureProjectHome?: (projectId: string) => Promise<void>;
}

export type { ProjectHomeInfo } from "../../utils/projectHomeGroups";

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
  crossHostSessionsVisible,
  hostLabelForInstance = (instanceId) => instanceId,
  sessionMetadataBySessionId = new Map(),
  selectedForDelete,
  onToggleSelect,
  selectionMode = false,
  onEnterSelectionMode,
  onExitSelectionMode,
  onSelectAll,
  onBulkDelete,
  notices,
  projects = [],
  projectHomes = [],
  onEnsureProjectHome,
}: SessionDrawerProps) {
  const owningHost: OwningHostInfo = {
    selectedInstanceId,
    hostLabelForInstance,
  };
  const [collapsedProjects, setCollapsedProjects] = useState<Set<string>>(readCollapsedProjects);
  // A project with a home agent but no session yet still gets its group, so its pinned entry is
  // reachable; only when the pinned entry can act at all.
  const projectGroups = withProjectHomeGroups(
    groupSessionsByProject(sessions, projects),
    projects,
    onEnsureProjectHome ? projectHomes : [],
  );
  const selectedGroupKey = projectGroupKeyContaining(projectGroups, selectedSessionId);

  // Selecting a session expands its project group; the viewer can still collapse it afterwards.
  useEffect(() => {
    if (selectedGroupKey === null) return;
    setCollapsedProjects((prev) => {
      if (!prev.has(selectedGroupKey)) return prev;
      const next = new Set(prev);
      next.delete(selectedGroupKey);
      return next;
    });
  }, [selectedGroupKey]);

  useEffect(() => {
    writeCollapsedProjects(collapsedProjects);
  }, [collapsedProjects]);

  const toggleProjectGroup = (key: string) => {
    setCollapsedProjects((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
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
        <style>{SESSION_INDICATOR_DOT_STYLES}</style>
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
          {sessions.map((session) => (
            <CollapsedSessionDot
              key={session.sessionId}
              session={session}
              isSelected={session.sessionId === selectedSessionId}
              onSelectSession={onSelectSession}
            />
          ))}
        </div>
      </div>
    );
  }

  const partitionBodyProps = {
    selectedSessionId,
    onSelectSession,
    owningHost,
    sessionMetadataBySessionId,
    selectedForDelete,
    onToggleSelect,
  };
  const selectedCount = selectedForDelete?.size ?? 0;
  const allSelected =
    sessions.length > 0 && sessions.every((s) => selectedForDelete?.has(s.sessionId));

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
      <style>{SESSION_INDICATOR_DOT_STYLES}</style>
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
      {notices && <div className="px-3 py-2 border-b border-border space-y-1">{notices}</div>}
      <ScrollArea className="flex-1 min-h-0">
        <div className="py-1 px-2 space-y-0.5">
          {projectGroups.map((group) => {
            const key = projectGroupKey(group);
            const { home, homeSession, rows } = projectHomeSlot(group, projectHomes);
            return (
              <SessionProjectGroup
                key={key}
                group={group}
                collapsed={collapsedProjects.has(key)}
                onToggle={() => toggleProjectGroup(key)}
              >
                {home && onEnsureProjectHome && (
                  <ProjectHomeEntry
                    home={home}
                    homeSession={homeSession}
                    selected={homeSession !== null && homeSession.sessionId === selectedSessionId}
                    onEnsure={onEnsureProjectHome}
                  />
                )}
                <SessionGroupBody
                  sessions={rows}
                  selectionMode={selectionMode}
                  partitionBodyProps={partitionBodyProps}
                />
              </SessionProjectGroup>
            );
          })}
          {sessions.length === 0 && (
            <div className="px-3 py-4 text-sm text-muted-foreground text-center">
              No sessions
            </div>
          )}
          {/* Under the rows, not over them: the list is still the answer to "what is running", and
              this is the footnote saying which part of the question it could not reach. */}
          {!crossHostSessionsVisible && (
            <div
              data-testid="sessions-drawer-cross-host-unavailable"
              className="border-t border-border px-3 py-2 text-xs text-muted-foreground"
            >
              Sessions on other hosts are not visible from this connection.
            </div>
          )}
        </div>
      </ScrollArea>
      {sessions.length > 0 && (
        <div
          data-testid="sessions-drawer-select-bar"
          className="flex items-center gap-1 border-t border-border px-2 py-1.5"
        >
          {selectionMode ? (
            <>
              <button
                type="button"
                data-testid="sessions-drawer-select-all"
                onClick={onSelectAll}
                className="rounded px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              >
                {allSelected ? "Deselect all" : "Select all"}
              </button>
              <span className="flex-1 text-center text-xs text-muted-foreground">
                {selectedCount} selected
              </span>
              <button
                type="button"
                data-testid="sessions-drawer-bulk-delete"
                onClick={onBulkDelete}
                disabled={selectedCount === 0}
                className="rounded px-1.5 py-1 text-xs text-destructive transition-colors hover:bg-destructive/10 disabled:pointer-events-none disabled:opacity-40"
              >
                Delete
              </button>
              <button
                type="button"
                data-testid="sessions-drawer-select-cancel"
                onClick={onExitSelectionMode}
                className="rounded px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              >
                Cancel
              </button>
            </>
          ) : (
            <button
              type="button"
              data-testid="sessions-drawer-select-mode"
              onClick={onEnterSelectionMode}
              className="rounded px-1.5 py-1 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              Select
            </button>
          )}
        </div>
      )}
    </div>
  );
}
