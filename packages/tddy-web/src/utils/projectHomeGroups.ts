import type { ProjectEntry } from "../gen/project_pb";
import type { SessionEntry } from "../gen/session_pb";
import type { KnownProject, ProjectSessionGroup } from "./sessionProjectGroups";

/** One project's pinned home entry, as the drawer renders it (`#project-home` 4/7). */
export interface ProjectHomeInfo {
  projectId: string;
  mainBranchRef: string;
  /**
   * The session the project records as its main (`ProjectEntry.homeSessionId`); `null` when main
   * has not been set up. Configured iff non-null.
   */
  homeSessionId: string | null;
}

/**
 * Each project's main as recorded on `instanceId` — the host the pinned entry ensures main on.
 * `home_session_id` is per host and `ListProjects` returns one row per host a project is registered
 * on, so only that host's rows count: another host's row would show its main (or its lack of one)
 * for an entry that acts on this one. A project with no row on that host has no main there to
 * reach, and gets no home. No host selected means nowhere to ensure main, so no homes at all.
 */
export function projectHomesOnHost(
  projects: ReadonlyArray<ProjectEntry>,
  instanceId: string | null,
): ProjectHomeInfo[] {
  if (instanceId === null) return [];
  return projects
    .filter((p) => p.daemonInstanceId === instanceId)
    .map((p) => ({
      projectId: p.projectId,
      mainBranchRef: p.mainBranchRef,
      homeSessionId: p.homeSessionId === "" ? null : p.homeSessionId,
    }));
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
 * project's recorded main, so that session is not repeated as a row. Any other session carrying
 * the project-home role — a main that was replaced — is an ordinary row. The **Other** group, and
 * a project absent from `homes`, have no pinned entry and keep every session as a row.
 */
export function projectHomeSlot(
  group: ProjectSessionGroup,
  homes: ReadonlyArray<ProjectHomeInfo>,
): ProjectHomeSlot {
  const home =
    group.projectId === null ? undefined : homes.find((h) => h.projectId === group.projectId);
  if (!home) return { home, homeSession: null, rows: group.sessions };
  const isHome = (s: SessionEntry) =>
    home.homeSessionId !== null && s.sessionId === home.homeSessionId;
  return {
    home,
    homeSession: group.sessions.find((s) => isHome(s)) ?? null,
    rows: group.sessions.filter((s) => !isHome(s)),
  };
}

/**
 * Adds an empty group for each known project that has a main but no listed session yet, so its
 * pinned **main** entry — the way to start that project's home session — is reachable before the
 * project has any session (`#project-home` 4/7). The session grouping itself
 * (`groupSessionsByProject`, `#project-home` 2/7) is untouched: this only fills in the groups it
 * leaves out, ordered among the others by the same name rule, ahead of the trailing **Other** group.
 *
 * A project without a main gets no empty group: its entry could only show a hint.
 */
export function withProjectHomeGroups(
  groups: ProjectSessionGroup[],
  projects: ReadonlyArray<KnownProject>,
  homes: ReadonlyArray<Pick<ProjectHomeInfo, "projectId" | "homeSessionId">>,
): ProjectSessionGroup[] {
  const grouped = new Set(groups.map((g) => g.projectId));
  const startable = new Set(homes.filter((h) => h.homeSessionId !== null).map((h) => h.projectId));
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
