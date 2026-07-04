import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService, type ProjectEntry } from "../../gen/connection_pb";
import { useAuth } from "../../hooks/useAuth";
import { useDaemonClient, useDaemons } from "../../rpc/selectedDaemon";
import { DaemonSelectorConnected } from "../shell/DaemonSelector";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { ProjectsScreen } from "./ProjectsScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const POLL_INTERVAL_MS = 5000;

/**
 * Polls the project registry and wires create-project + add-to-host RPCs over the given
 * daemon-level `ConnectionService` client (`null` until a daemon is selected — every call site
 * skips rather than throwing or faking success; see `useDaemonClient`'s contract).
 */
function useProjectsRpc(client: Client<typeof ConnectionService> | null, sessionToken: string) {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);

  const loadProjects = useCallback(() => {
    if (!client) return;
    client
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => {});
  }, [client, sessionToken]);

  useEffect(() => {
    loadProjects();
    const id = setInterval(loadProjects, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [loadProjects]);

  const createProject = useCallback(
    (input: { name: string; gitUrl: string; userRelativePath: string }) => {
      if (!client) return;
      client
        .createProject({ sessionToken, ...input })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  const addProjectToHost = useCallback(
    (input: { projectId: string; name: string; gitUrl: string; daemonInstanceId: string }) => {
      if (!client) return;
      client
        .addProjectToHost({ sessionToken, mainBranchRef: "", userRelativePath: "", ...input })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  return { projects, createProject, addProjectToHost };
}

/**
 * Data container for the dedicated Projects screen (`/projects`). RPC wiring lives in
 * {@link useProjectsRpc}, over the shared common-room daemon-level RPC client (`useDaemonClient`,
 * see `SelectedDaemonProvider`). The selectable hosts are the **daemon-role** participants
 * currently in the common LiveKit room, sourced from the same shared context (`useDaemons`); only
 * daemons own projects, so coder/browser participants are never offered as hosts.
 */
export function ProjectsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { user, logout } = useAuth();
  // Read the token directly (like SessionsDrawerScreen) so project RPCs fire independent of the
  // auth-status round-trip.
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";
  const client = useDaemonClient(ConnectionService);
  const daemons = useDaemons();
  const { projects, createProject, addProjectToHost } = useProjectsRpc(client, sessionToken);

  return (
    <div className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">Projects</h1>
        <DaemonSelectorConnected />
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>

      <ProjectsScreen
        projects={projects}
        daemons={daemons}
        onCreateProject={createProject}
        onAddProjectToHost={addProjectToHost}
      />
    </div>
  );
}
