import type { SessionEntry } from "../gen/session_pb";
import type { KnownProject, ProjectSessionGroup } from "./sessionProjectGroups";

/** One project's pinned home entry, as the drawer renders it (`#project-home` 4/7). */
export interface ProjectHomeInfo {
  projectId: string;
  mainBranchRef: string;
  /** The home agent's label; `null` when no home agent is set. */
  homeAgentLabel: string | null;
}

/** `SessionEntry.sessionRole` of a project's project-home session. */
export const PROJECT_HOME_ROLE = "project-home";

/** What one project group renders: its pinned entry (if any), the home session, the other rows. */
export interface ProjectHomeSlot {
  home: ProjectHomeInfo | undefined;
  homeSession: SessionEntry | null;
  rows: SessionEntry[];
}

/**
 * Splits a group into its pinned home entry and its rows. The pinned entry stands for the
 * project-home session, so that session is not repeated as a row. The **Other** group, and a
 * project absent from `homes`, have no pinned entry and keep every session as a row.
 */
export function projectHomeSlot(
  group: ProjectSessionGroup,
  homes: ReadonlyArray<ProjectHomeInfo>,
): ProjectHomeSlot {
  const home =
    group.projectId === null ? undefined : homes.find((h) => h.projectId === group.projectId);
  if (!home) return { home, homeSession: null, rows: group.sessions };
  return {
    home,
    homeSession: group.sessions.find((s) => s.sessionRole === PROJECT_HOME_ROLE) ?? null,
    rows: group.sessions.filter((s) => s.sessionRole !== PROJECT_HOME_ROLE),
  };
}

/**
 * Adds an empty group for each known project that has a home agent but no session yet, so its
 * pinned **main** entry — the way to start that project's home session — is reachable before the
 * project has any session (`#project-home` 4/7). The session grouping itself
 * (`groupSessionsByProject`, `#project-home` 2/7) is untouched: this only fills in the groups it
 * leaves out, ordered among the others by the same name rule, ahead of the trailing **Other** group.
 *
 * A project without a home agent gets no empty group: its entry could only show a hint.
 */
export function withProjectHomeGroups(
  groups: ProjectSessionGroup[],
  projects: ReadonlyArray<KnownProject>,
  homes: ReadonlyArray<Pick<ProjectHomeInfo, "projectId" | "homeAgentLabel">>,
): ProjectSessionGroup[] {
  const grouped = new Set(groups.map((g) => g.projectId));
  const startable = new Set(homes.filter((h) => h.homeAgentLabel !== null).map((h) => h.projectId));
  const added: ProjectSessionGroup[] = projects
    .filter((p) => startable.has(p.projectId) && !grouped.has(p.projectId))
    .map((p) => ({ projectId: p.projectId, label: p.name, sessions: [] }));
  if (added.length === 0) return groups;

  const other = groups.filter((g) => g.projectId === null);
  const named = [...groups.filter((g) => g.projectId !== null), ...added].sort(
    (a, b) =>
      a.label.localeCompare(b.label, undefined, { sensitivity: "base" }) ||
      (a.projectId ?? "").localeCompare(b.projectId ?? ""),
  );
  return [...named, ...other];
}
