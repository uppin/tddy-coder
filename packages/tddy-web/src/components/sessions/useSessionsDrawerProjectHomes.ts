import { useMemo } from "react";
import type { ProjectEntry } from "../../gen/project_pb";
import type { ProjectHomeInfo } from "../../utils/projectHomeGroups";

/**
 * What each project's pinned **main** entry stands for (`#project-home` 4/7): its main branch ref
 * and the session the project records as its main, read from `ListProjects` rows. An empty
 * `homeSessionId` — main not set up — is `null`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */
export function useSessionsDrawerProjectHomes(
  projects: ReadonlyArray<ProjectEntry>,
): ProjectHomeInfo[] {
  return useMemo(
    () =>
      projects.map((p) => ({
        projectId: p.projectId,
        mainBranchRef: p.mainBranchRef,
        homeSessionId: p.homeSessionId === "" ? null : p.homeSessionId,
      })),
    [projects],
  );
}
