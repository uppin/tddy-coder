import { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { CatalogService } from "../../gen/catalog_pb";
import type { ProjectEntry } from "../../gen/project_pb";
import type { ProjectHomeInfo } from "./SessionDrawer";

/**
 * What each project's pinned **main** entry shows (`#project-home` 4/7): its main branch ref and
 * its home agent's label, read from `ListProjects` rows and the daemon's agent list. A home agent
 * the list does not name shows under its id; one that is not set is `null`.
 */
export function useSessionsDrawerProjectHomes(
  catalogClient: Client<typeof CatalogService> | null,
  projects: ReadonlyArray<ProjectEntry>,
): ProjectHomeInfo[] {
  const [labelById, setLabelById] = useState<ReadonlyMap<string, string>>(new Map());
  useEffect(() => {
    if (!catalogClient) {
      setLabelById(new Map());
      return;
    }
    catalogClient
      .listAgents({})
      .then((res) => setLabelById(new Map(res.agents.map((a) => [a.id, a.label]))))
      .catch(() => setLabelById(new Map()));
  }, [catalogClient]);

  return useMemo(
    () =>
      projects.map((p) => ({
        projectId: p.projectId,
        mainBranchRef: p.mainBranchRef,
        homeAgentLabel: p.homeAgent ? (labelById.get(p.homeAgent) ?? p.homeAgent) : null,
      })),
    [projects, labelById],
  );
}
