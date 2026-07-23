import { useCallback, useEffect, useState } from "react";
import { createClient, type Client } from "@connectrpc/connect";
import { ConnectionService, type ProjectEntry } from "../../gen/connection_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useDaemonClient, useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useLiveKitTransportFactory } from "../../rpc/transportProvider";
import { daemonRpcIdentity } from "../../lib/participantRole";
import { DaemonSelectorConnected } from "../shell/DaemonSelector";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { ProjectsScreen } from "./ProjectsScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const POLL_INTERVAL_MS = 5000;

/**
 * Polls the project registry over the selected-daemon `client` (list/create), and wires the
 * add-to-host RPC over a client addressed to the **chosen target host** (`clientForHost`) so the
 * request reaches that daemon directly rather than double-hopping through the selected daemon.
 * Every call site skips when its client is `null` (no daemon selected / room not connected) rather
 * than throwing or faking success; see `useDaemonClient`'s contract.
 */
function useProjectsRpc(
  client: Client<typeof ConnectionService> | null,
  clientForHost: (instanceId: string) => Client<typeof ConnectionService> | null,
  sessionToken: string,
) {
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
    (input: {
      projectId: string;
      name: string;
      gitUrl: string;
      daemonInstanceId: string;
      userRelativePath: string;
    }) => {
      const target = clientForHost(input.daemonInstanceId);
      if (!target) return;
      target
        .addProjectToHost({ sessionToken, mainBranchRef: "", ...input })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [clientForHost, sessionToken, loadProjects],
  );

  const setDefaultBranch = useCallback(
    (input: { projectId: string; mainBranchRef: string; daemonInstanceId: string }) => {
      if (!client) return;
      client
        .setProjectDefaultBranch({ sessionToken, ...input })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  const loadProjectBranches = useCallback(
    async (input: { projectId: string; daemonInstanceId: string }): Promise<string[]> => {
      if (!client) return [];
      const res = await client.listProjectBranches({ sessionToken, ...input });
      return res.branches;
    },
    [client, sessionToken],
  );

  return { projects, createProject, addProjectToHost, setDefaultBranch, loadProjectBranches };
}

/**
 * Data container for the dedicated Projects screen (`/projects`). RPC wiring lives in
 * {@link useProjectsRpc}, over the shared common-room daemon-level RPC client (`useDaemonClient`,
 * see `SelectedDaemonProvider`). The selectable hosts are the **daemon-role** participants
 * currently in the common LiveKit room, sourced from the same shared context (`useDaemons`); only
 * daemons own projects, so coder/browser participants are never offered as hosts.
 */
export function ProjectsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { user, logout, sessionToken } = useAuthContext();
  const client = useDaemonClient(ConnectionService);
  const daemons = useDaemons();
  const { room } = useSelectedDaemon();
  const liveKitFactory = useLiveKitTransportFactory();
  // Address the chosen target host directly (`daemon-{instanceId}`) over the shared common-room
  // connection — the target is only known when the operator submits, so the client is built here
  // from the room + transport factory rather than a render-time `useDaemonClientFor` hook.
  const clientForHost = useCallback(
    (instanceId: string): Client<typeof ConnectionService> | null =>
      room
        ? createClient(ConnectionService, liveKitFactory(room, daemonRpcIdentity(instanceId)))
        : null,
    [room, liveKitFactory],
  );
  const { projects, createProject, addProjectToHost, setDefaultBranch, loadProjectBranches } =
    useProjectsRpc(client, clientForHost, sessionToken ?? "");

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
        onSetDefaultBranch={setDefaultBranch}
        loadProjectBranches={loadProjectBranches}
      />
    </div>
  );
}
