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

/** True when the session is not listed under any project accordion (id or repo match). */
export function isSessionOrphan(s: SessionEntry, projects: ProjectEntry[]): boolean {
  if (s.projectId.trim() !== "") {
    return !projects.some((p) => p.projectId === s.projectId);
  }
  return projectForUnscopedSession(s, projects) === undefined;
}
