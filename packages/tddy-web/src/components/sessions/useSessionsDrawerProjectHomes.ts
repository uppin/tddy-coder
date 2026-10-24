import { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { CatalogService, type AgentInfo } from "../../gen/catalog_pb";
import type { ProjectEntry } from "../../gen/project_pb";
import type { ProjectHomeInfo } from "../../utils/projectHomeGroups";

/**
 * What each project's pinned **main** entry shows (`#project-home` 4/7): its main branch ref and
 * its home agent's label, read from `ListProjects` rows and the daemon's agent list. A home agent
 * the list does not name shows under its id; one that is not set is `null`. The agent list itself
 * comes back too — it is what the setup pane offers a project with no home agent.
 */
export function useSessionsDrawerProjectHomes(
  catalogClient: Client<typeof CatalogService> | null,
  projects: ReadonlyArray<ProjectEntry>,
): { homes: ProjectHomeInfo[]; agents: AgentInfo[] } {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  useEffect(() => {
    if (!catalogClient) {
      setAgents([]);
      return;
    }
    catalogClient
      .listAgents({})
      .then((res) => setAgents(res.agents))
      .catch(() => setAgents([]));
  }, [catalogClient]);

  const homes = useMemo(() => {
    const labelById = new Map(agents.map((a) => [a.id, a.label]));
    return projects.map((p) => ({
      projectId: p.projectId,
      mainBranchRef: p.mainBranchRef,
      homeAgentLabel: p.homeAgent ? (labelById.get(p.homeAgent) ?? p.homeAgent) : null,
    }));
  }, [projects, agents]);

  return { homes, agents };
}
