import type { SessionEntry } from "../gen/session_pb";

/** One project's slice of the drawer: its header label and the sessions under it. */
export interface ProjectSessionGroup {
  /** The project id, or `null` for the trailing **Other** group. */
  projectId: string | null;
  /** The project's name; `"Other"` for the trailing group. */
  label: string;
  /** The group's sessions, in the order they arrived. */
  sessions: SessionEntry[];
}

/** A project the drawer knows by name (a `ListProjects` row, reduced). */
export interface KnownProject {
  projectId: string;
  name: string;
}

/**
 * Groups sessions per project for the drawer (`#project-home` 2/7).
 *
 * - One group per known project that has at least one session, ordered by name
 *   (case-insensitive).
 * - Sessions with an empty `projectId`, or one no known project carries, go in a trailing
 *   **Other** group, which is absent when empty.
 * - Inside a group, sessions keep their input order — partitioning and stack nesting are the
 *   caller's, applied per group.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-22-project-home-project-groups.md.
 */
export function groupSessionsByProject(
  sessions: SessionEntry[],
  projects: ReadonlyArray<KnownProject>,
): ProjectSessionGroup[] {
  const knownNames = new Map(projects.map((p) => [p.projectId, p.name]));
  const byProject = new Map<string, SessionEntry[]>();
  const other: SessionEntry[] = [];

  for (const session of sessions) {
    if (session.projectId && knownNames.has(session.projectId)) {
      const bucket = byProject.get(session.projectId) ?? [];
      bucket.push(session);
      byProject.set(session.projectId, bucket);
    } else {
      other.push(session);
    }
  }

  const groups: ProjectSessionGroup[] = [...byProject.entries()]
    .map(([projectId, groupSessions]) => ({
      projectId,
      label: knownNames.get(projectId) ?? projectId,
      sessions: groupSessions,
    }))
    .sort(
      (a, b) =>
        a.label.localeCompare(b.label, undefined, { sensitivity: "base" }) ||
        a.projectId.localeCompare(b.projectId),
    );

  if (other.length > 0) {
    groups.push({ projectId: null, label: "Other", sessions: other });
  }
  return groups;
}
