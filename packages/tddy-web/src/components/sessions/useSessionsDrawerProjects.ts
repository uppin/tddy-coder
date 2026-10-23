import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ProjectService, type ProjectEntry } from "../../gen/project_pb";
import { useSelectedDaemon } from "../../rpc/selectedDaemon";
import { hostNotConnected, rpcFailureText } from "../../rpc/rpcFailure";

/** The registry the drawer read, and why the last read failed (`null` while it has not). */
export interface SessionsDrawerProjects {
  projects: ProjectEntry[];
  error: string | null;
}

/**
 * The project registry (daemon-level RPC over the selected-daemon common-room connection). Used
 * to resolve an unscoped session's project from its `repoPath` before the worktree RPCs — those
 * require a non-empty `project_id`. A failed read keeps the last good list and reports why in
 * `error`, as does having no client at all (no daemon selected, or no wire reaches it): an empty
 * registry would otherwise leave unscoped sessions without a project and nothing saying why.
 */
export function useSessionsDrawerProjects(
  projectClient: Client<typeof ProjectService> | null,
  sessionToken: string,
): SessionsDrawerProjects {
  const { selectedInstanceId, selectedLabel } = useSelectedDaemon();
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!projectClient) {
      setError(rpcFailureText(hostNotConnected(selectedLabel ?? selectedInstanceId)));
      return;
    }
    let cancelled = false;
    projectClient
      .listProjects({ sessionToken })
      .then((res) => {
        if (cancelled) return;
        setProjects(res.projects);
        setError(null);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(rpcFailureText(e));
      });
    return () => {
      cancelled = true;
    };
  }, [projectClient, sessionToken, selectedLabel, selectedInstanceId]);
  return { projects, error };
}
