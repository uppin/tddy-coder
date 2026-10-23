import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { type ProjectEntry } from "../../gen/project_pb";
import { ProjectService } from "../../gen/project_pb";
import { AccountsService } from "../../gen/accounts_pb";
import { HostService } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostConnector } from "../../rpc/connections/registry";
import { useDaemonClient, useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { hostNotConnected, rpcFailureText } from "../../rpc/rpcFailure";
import { AppShell } from "../shell/AppShell";
import { ProjectsScreen, type AssignableAccount } from "./ProjectsScreen";

const POLL_INTERVAL_MS = 5000;

/**
 * Polls the project registry over the selected-daemon `client` (list/create), and wires the
 * add-to-host RPC over a client addressed to the **chosen target host** (`clientForHost`) so the
 * request reaches that daemon directly rather than double-hopping through the selected daemon.
 *
 * Every action returns a promise that rejects with the reason it failed — the daemon's refusal, or
 * {@link hostNotConnected} when `client` is `null` (no daemon selected / no wire reaches it) — so
 * the screen can say why next to the control that was used. A failed poll is kept in `loadError`
 * rather than leaving the list looking empty; the last good list stays shown beside it.
 */
function useProjectsRpc(
  client: Client<typeof ProjectService> | null,
  clientForHost: (instanceId: string) => Client<typeof ProjectService> | null,
  sessionToken: string,
  hostLabelOf: (instanceId: string | null) => string | null,
  selectedInstanceId: string | null,
) {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const connected = useCallback((): Client<typeof ProjectService> => {
    if (!client) throw hostNotConnected(hostLabelOf(selectedInstanceId));
    return client;
  }, [client, hostLabelOf, selectedInstanceId]);

  const loadProjects = useCallback(() => {
    if (!client) {
      setLoadError(rpcFailureText(hostNotConnected(hostLabelOf(selectedInstanceId))));
      return;
    }
    client
      .listProjects({ sessionToken })
      .then((res) => {
        setProjects(res.projects);
        setLoadError(null);
      })
      .catch((e: unknown) => setLoadError(rpcFailureText(e)));
  }, [client, sessionToken, hostLabelOf, selectedInstanceId]);

  useEffect(() => {
    loadProjects();
    const id = setInterval(loadProjects, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [loadProjects]);

  const createProject = useCallback(
    async (input: { name: string; gitUrl: string; userRelativePath: string }) => {
      await connected().createProject({ sessionToken, ...input });
      loadProjects();
    },
    [connected, sessionToken, loadProjects],
  );

  const addProjectToHost = useCallback(
    async (input: {
      projectId: string;
      name: string;
      gitUrl: string;
      daemonInstanceId: string;
      userRelativePath: string;
    }) => {
      const target = clientForHost(input.daemonInstanceId);
      if (!target) throw hostNotConnected(hostLabelOf(input.daemonInstanceId));
      await target.addProjectToHost({ sessionToken, mainBranchRef: "", ...input });
      loadProjects();
    },
    [clientForHost, hostLabelOf, sessionToken, loadProjects],
  );

  const setDefaultBranch = useCallback(
    async (input: { projectId: string; mainBranchRef: string; daemonInstanceId: string }) => {
      await connected().setProjectDefaultBranch({ sessionToken, ...input });
      loadProjects();
    },
    [connected, sessionToken, loadProjects],
  );

  const loadProjectBranches = useCallback(
    async (input: { projectId: string; daemonInstanceId: string }): Promise<{ branches: string[]; defaultRemote: string }> => {
      const res = await connected().listProjectBranches({ sessionToken, ...input });
      return { branches: res.branches, defaultRemote: res.defaultRemote };
    },
    [connected, sessionToken],
  );

  const setProjectAccounts = useCallback(
    async (input: {
      projectId: string;
      accounts: { provider: string; accountId: string }[];
      localGh: boolean;
      daemonInstanceId: string;
    }) => {
      await connected().setProjectAccounts({ sessionToken, ...input });
      loadProjects();
    },
    [connected, sessionToken, loadProjects],
  );

  return {
    projects,
    loadError,
    createProject,
    addProjectToHost,
    setDefaultBranch,
    loadProjectBranches,
    setProjectAccounts,
  };
}

/**
 * The accounts the caller's vault holds, flattened from `ListAccounts`' provider groups. A locked or
 * absent vault yields no assignable accounts — the Accounts screen is where those states are
 * explained — but a failed read is returned as `error`, so empty pickers say why. No client is not
 * reported here: the project list already says the host is not connected.
 */
function useAssignableAccounts(sessionToken: string): { accounts: AssignableAccount[]; error: string | null } {
  const client = useDaemonClient(AccountsService);
  const [accounts, setAccounts] = useState<AssignableAccount[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!client) return;
    let current = true;
    client
      .listAccounts({ sessionToken })
      .then((res) => {
        if (!current) return;
        setError(null);
        setAccounts(
          res.providers.flatMap((group) =>
            group.accounts.map((a) => ({
              provider: group.provider,
              accountId: a.accountId,
              label: a.label,
            })),
          ),
        );
      })
      .catch((e: unknown) => {
        if (current) setError(rpcFailureText(e));
      });
    return () => {
      current = false;
    };
  }, [client, sessionToken]);

  return { accounts, error };
}

/**
 * Data container for the dedicated Projects screen (`/projects`). RPC wiring lives in
 * {@link useProjectsRpc}, over the daemon-level RPC client of the selected host (`useDaemonClient`,
 * see `SelectedDaemonProvider`). The selectable hosts are the **daemon-role** participants
 * currently in the common LiveKit room, sourced from the same shared context (`useDaemons`); only
 * daemons own projects, so coder/browser participants are never offered as hosts.
 */
export function ProjectsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { sessionToken } = useAuthContext();
  const client = useDaemonClient(ProjectService);
  const daemons = useDaemons();
  const connectHost = useHostConnector();
  // Address the chosen target host directly — the target is only known when the operator submits,
  // so the client is resolved here through the connection registry rather than by a render-time
  // `useDaemonClientFor` hook.
  const clientForHost = useCallback(
    (instanceId: string): Client<typeof ProjectService> | null =>
      connectHost(instanceId)?.clientFor(ProjectService) ?? null,
    [connectHost],
  );
  const { selectedInstanceId, selectedLabel } = useSelectedDaemon();
  // A host's label when it is listed, else its instance id — what a "not connected" error names.
  // Read through a ref: the daemon list is rebuilt on every participant event, and a callback that
  // changed with it would restart the project poll each time.
  const labelSourceRef = useRef({ daemons, selectedInstanceId, selectedLabel });
  labelSourceRef.current = { daemons, selectedInstanceId, selectedLabel };
  const hostLabelOf = useCallback((instanceId: string | null): string | null => {
    if (!instanceId) return null;
    const source = labelSourceRef.current;
    if (instanceId === source.selectedInstanceId && source.selectedLabel) return source.selectedLabel;
    return source.daemons.find((d) => d.instanceId === instanceId)?.label || instanceId;
  }, []);
  const { accounts, error: accountsLoadError } = useAssignableAccounts(sessionToken ?? "");
  const hostClient = useDaemonClient(HostService);
  // Each host is asked about its own `gh`: the serving daemon relays to `daemonInstanceId`.
  const loadHostGithubCli = useCallback(
    async (input: { daemonInstanceId: string; githubHostname: string }) => {
      if (!hostClient) throw new Error("no daemon selected to ask about the host's gh");
      const res = await hostClient.getHostTooling({ sessionToken: sessionToken ?? "", ...input });
      if (!res.githubCli) throw new Error("the host's tooling report carries no gh state");
      return res.githubCli;
    },
    [hostClient, sessionToken],
  );
  const {
    projects,
    loadError,
    createProject,
    addProjectToHost,
    setDefaultBranch,
    loadProjectBranches,
    setProjectAccounts,
  } = useProjectsRpc(client, clientForHost, sessionToken ?? "", hostLabelOf, selectedInstanceId);

  return (
    <AppShell title="Projects" onNavigate={onNavigate} variant="scroll" requiresHost>
      <ProjectsScreen
        projects={projects}
        loadError={loadError}
        accountsLoadError={accountsLoadError}
        daemons={daemons}
        onCreateProject={createProject}
        onAddProjectToHost={addProjectToHost}
        onSetDefaultBranch={setDefaultBranch}
        loadProjectBranches={loadProjectBranches}
        accounts={accounts}
        onSetProjectAccounts={setProjectAccounts}
        loadHostGithubCli={loadHostGithubCli}
      />
    </AppShell>
  );
}
