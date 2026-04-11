import type { ProjectEntry, SessionEntry } from "../gen/connection_pb";
import { sortSessionsForDisplay } from "./sessionSort";

function normalizeRepoPath(path: string): string {
  const t = path.trim();
  if (t.length <= 1) {
    return t;
  }
  return t.endsWith("/") ? t.slice(0, -1) : t;
}

/**
 * Stable UI / selection key for one registry row: `projectId` when `daemonInstanceId` is empty
 * (legacy single-daemon), else `projectId__daemonInstanceId`.
 */
export function connectionProjectRowKey(p: ProjectEntry): string {
  const d = (p.daemonInstanceId ?? "").trim();
  return d.length > 0 ? `${p.projectId}__${d}` : p.projectId;
}

/**
 * True when an unscoped session's repo path is the same checkout as `mainRepoPath`, or lives
 * under it (e.g. git worktree under the main clone).
 */
function repoPathMatchesProjectMain(sessionRepo: string, mainRepo: string): boolean {
  const s = normalizeRepoPath(sessionRepo);
  const m = normalizeRepoPath(mainRepo);
  if (s === m) {
    return true;
  }
  if (m.length <= 1 || s.length <= m.length) {
    return false;
  }
  return s.startsWith(`${m}/`);
}

/**
 * For sessions with empty `projectId`, picks the registered project whose `mainRepoPath` is the
 * longest prefix match (exact or parent directory). Avoids attaching `/a/b` to project `/a` when
 * `/a/b` is also registered.
 */
export function projectForUnscopedSession(
  session: SessionEntry,
  projects: ProjectEntry[]
): ProjectEntry | undefined {
  if (session.projectId.trim() !== "") {
    return undefined;
  }
  let best: ProjectEntry | undefined;
  let bestMainLen = -1;
  for (const p of projects) {
    const mp = normalizeRepoPath(p.mainRepoPath);
    if (!repoPathMatchesProjectMain(session.repoPath, p.mainRepoPath)) {
      continue;
    }
    if (mp.length > bestMainLen) {
      bestMainLen = mp.length;
      best = p;
    }
  }
  return best;
}

export function sessionBelongsToProject(
  session: SessionEntry,
  project: ProjectEntry,
  projects: ProjectEntry[]
): boolean {
  const pid = session.projectId.trim();
  if (pid !== "") {
    return session.projectId === project.projectId;
  }
  const resolved = projectForUnscopedSession(session, projects);
  return resolved !== undefined && resolved.projectId === project.projectId;
}

/**
 * Sessions shown under a project accordion: same `projectId`, or unscoped (`projectId` empty
 * after trim) with `repoPath` matching the project's `mainRepoPath` (trim; trailing `/` ignored).
 */
export function sortedSessionsForProjectTable(
  sessions: SessionEntry[],
  project: ProjectEntry,
  allProjects: ProjectEntry[]
): SessionEntry[] {
  return sortSessionsForDisplay(
    sessions.filter((s) => sessionBelongsToProject(s, project, allProjects))
  );
}

/** Projects on the same host as `session` (by `daemon_instance_id`). */
function projectsOnSessionHost(session: SessionEntry, projects: ProjectEntry[]): ProjectEntry[] {
  const sid = (session.daemonInstanceId ?? "").trim();
  return projects.filter((p) => {
    const pd = (p.daemonInstanceId ?? "").trim();
    if (pd === "") {
      return sid === "";
    }
    return pd === sid;
  });
}

/** True when the session is not listed under any project accordion (id or repo match on its host). */
export function isSessionOrphan(s: SessionEntry, projects: ProjectEntry[]): boolean {
  const onHost = projectsOnSessionHost(s, projects);
  if (s.projectId.trim() !== "") {
    return !onHost.some((p) => p.projectId === s.projectId);
  }
  return projectForUnscopedSession(s, onHost) === undefined;
}

/**
 * Whether `session` belongs in the table for (`project`, `hostingDaemonInstanceId`).
 * Scoped sessions must match `project_id` and owning daemon. Unscoped sessions resolve only
 * against `projectsOnHost` and must not attach to a sibling host's copy of the same project id.
 */
export function sessionBelongsToProjectHost(
  session: SessionEntry,
  project: ProjectEntry,
  hostingDaemonInstanceId: string,
  projectsOnHost: ProjectEntry[]
): boolean {
  const hid = hostingDaemonInstanceId.trim();
  const sid = (session.daemonInstanceId ?? "").trim();
  const pd = (project.daemonInstanceId ?? "").trim();

  if (hid !== pd) {
    return false;
  }
  if (pd === "" && hid === "") {
    if (sid !== "") {
      return false;
    }
  } else if (sid !== hid) {
    return false;
  }

  const spid = session.projectId.trim();
  if (spid !== "") {
    return session.projectId === project.projectId;
  }
  const resolved = projectForUnscopedSession(session, projectsOnHost);
  return resolved !== undefined && resolved.projectId === project.projectId;
}

/** Sessions for one project accordion keyed by (project, hosting daemon). */
export function sortedSessionsForProjectHostTable(
  sessions: SessionEntry[],
  project: ProjectEntry,
  hostingDaemonInstanceId: string,
  projectsOnHost: ProjectEntry[]
): SessionEntry[] {
  return sortSessionsForDisplay(
    sessions.filter((s) =>
      sessionBelongsToProjectHost(s, project, hostingDaemonInstanceId, projectsOnHost)
    )
  );
}
