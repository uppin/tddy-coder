import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ProjectService, type ProjectEntry } from "../../gen/project_pb";

/**
 * The project registry (daemon-level RPC over the selected-daemon common-room connection). Used
 * to resolve an unscoped session's project from its `repoPath` before the worktree RPCs — those
 * require a non-empty `project_id`. Falls back to an empty list when no daemon is selected yet or
 * the call fails, so the drawer still renders.
 */
export function useSessionsDrawerProjects(
  projectClient: Client<typeof ProjectService> | null,
  sessionToken: string,
): ProjectEntry[] {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  useEffect(() => {
    if (!projectClient) {
      setProjects([]);
      return;
    }
    projectClient
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => setProjects([]));
  }, [projectClient, sessionToken]);
  return projects;
}
