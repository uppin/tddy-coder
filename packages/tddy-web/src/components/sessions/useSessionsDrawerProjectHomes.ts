import { useMemo } from "react";
import type { ProjectEntry } from "../../gen/project_pb";
import { projectHomesOnHost, type ProjectHomeInfo } from "../../utils/projectHomeGroups";

/** What the drawer's pinned **main** entries stand for, and the registry row setup reads. */
export interface SessionsDrawerProjectHomes {
  homes: ProjectHomeInfo[];
  /** The project's `ListProjects` row on the host main is ensured on; `null` when it has none. */
  projectOnHost: (projectId: string) => ProjectEntry | null;
}

/**
 * What each project's pinned **main** entry stands for (`#project-home` 4/7): its main branch ref
 * and the session the project records as its main, read from the `ListProjects` rows of
 * `instanceId` — the host the entry ensures main on and the setup pane starts it on (see
 * `projectHomesOnHost`). An empty `homeSessionId` — main not set up — is `null`.
 *
 * PRD: docs/ft/web/session-drawer.md
 */
export function useSessionsDrawerProjectHomes(
  projects: ReadonlyArray<ProjectEntry>,
  instanceId: string | null,
): SessionsDrawerProjectHomes {
  return useMemo(
    () => ({
      homes: projectHomesOnHost(projects, instanceId),
      projectOnHost: (projectId: string) =>
        projects.find((p) => p.projectId === projectId && p.daemonInstanceId === instanceId) ?? null,
    }),
    [projects, instanceId],
  );
}
