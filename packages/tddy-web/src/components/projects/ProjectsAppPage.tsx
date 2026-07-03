import { useCallback, useEffect, useMemo, useState } from "react";
import { ConnectionService, type ProjectEntry } from "../../gen/connection_pb";
import { useAuth } from "../../hooks/useAuth";
import { useCommonRoom } from "../../hooks/useCommonRoom";
import { useRoomParticipants } from "../../hooks/useRoomParticipants";
import { daemonHostsFromParticipants } from "../../lib/participantRole";
import { presenceIdentityForUser } from "../../lib/presenceIdentity";
import { useHttpClient } from "../../rpc/transportProvider";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { ProjectsScreen } from "./ProjectsScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

const POLL_INTERVAL_MS = 5000;

/**
 * Data container for the dedicated Projects screen (`/projects`). Polls the project registry and
 * wires create-project + add-to-host RPCs. The selectable hosts are the **daemon-role** participants
 * currently in the common LiveKit room (derived via {@link daemonHostsFromParticipants}); only
 * daemons own projects, so coder/browser participants are never offered as hosts.
 */
export function ProjectsAppPage({
  livekitUrl,
  commonRoom,
  onNavigate,
}: {
  livekitUrl?: string;
  commonRoom?: string;
  onNavigate: (path: string) => void;
}) {
  const { user, isAuthenticated, logout } = useAuth();
  // Read the token directly (like SessionsDrawerScreen) so project RPCs fire independent of the
  // auth-status round-trip.
  const sessionToken =
    typeof window !== "undefined"
      ? (window.localStorage.getItem("tddy_session_token") ?? "")
      : "";
  const client = useHttpClient(ConnectionService);

  const [projects, setProjects] = useState<ProjectEntry[]>([]);

  // Hosts come from the common-room presence: only genuine daemons (advertisement metadata),
  // excluding this browser and any coder/session participants.
  const identity = useMemo(
    () => (user ? presenceIdentityForUser(user.login) : undefined),
    [user],
  );
  const { room } = useCommonRoom(livekitUrl, commonRoom, isAuthenticated ? identity : undefined);
  const participants = useRoomParticipants(room);
  const daemons = useMemo(() => daemonHostsFromParticipants(participants), [participants]);

  const loadProjects = useCallback(() => {
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
