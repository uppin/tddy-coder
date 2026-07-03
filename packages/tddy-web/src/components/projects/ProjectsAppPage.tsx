import { useCallback, useEffect, useState } from "react";
import {
  ConnectionService,
  type EligibleDaemonEntry,
  type ProjectEntry,
} from "../../gen/connection_pb";
import { useAuth } from "../../hooks/useAuth";
import { useHttpClient } from "../../rpc/transportProvider";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { ProjectsScreen } from "./ProjectsScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const POLL_INTERVAL_MS = 5000;

/**
 * Data container for the dedicated Projects screen (`/projects`). Polls the project registry and
 * the connected-daemon (host) list, and wires create-project + add-to-host RPCs.
 */
export function ProjectsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { user, logout } = useAuth();
  // Read the token directly (like SessionsDrawerScreen) so project RPCs fire independent of the
  // auth-status round-trip.
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";
  const client = useHttpClient(ConnectionService);

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [daemons, setDaemons] = useState<EligibleDaemonEntry[]>([]);

  const loadProjects = useCallback(() => {
    client
      .listProjects({ sessionToken })
      .then((res) => setProjects(res.projects))
      .catch(() => {});
  }, [client, sessionToken]);

  const loadDaemons = useCallback(() => {
    client
      .listEligibleDaemons({ sessionToken })
      .then((res) => setDaemons(res.daemons))
      .catch(() => {});
  }, [client, sessionToken]);

  useEffect(() => {
    loadProjects();
    loadDaemons();
    const id = setInterval(() => {
      loadProjects();
      loadDaemons();
    }, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [loadProjects, loadDaemons]);

  const handleCreateProject = useCallback(
    (input: { name: string; gitUrl: string; userRelativePath: string }) => {
      client
        .createProject({
          sessionToken,
          name: input.name,
          gitUrl: input.gitUrl,
          userRelativePath: input.userRelativePath,
        })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  const handleAddProjectToHost = useCallback(
    (input: { projectId: string; name: string; gitUrl: string; daemonInstanceId: string }) => {
      client
        .addProjectToHost({
          sessionToken,
          projectId: input.projectId,
          name: input.name,
          gitUrl: input.gitUrl,
          mainBranchRef: "",
          daemonInstanceId: input.daemonInstanceId,
          userRelativePath: "",
        })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  return (
    <div className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">Projects</h1>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>

      <ProjectsScreen
        projects={projects}
        daemons={daemons}
        onCreateProject={handleCreateProject}
        onAddProjectToHost={handleAddProjectToHost}
      />
    </div>
  );
}
