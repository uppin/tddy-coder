import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { type ProjectEntry } from "../../gen/project_pb";
import { ProjectService } from "../../gen/project_pb";
import { AccountsService } from "../../gen/accounts_pb";
import { CatalogService, type AgentInfo } from "../../gen/catalog_pb";
import { HostService } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostConnector } from "../../rpc/connections/registry";
import { useDaemonClient, useDaemons } from "../../rpc/selectedDaemon";
import { AppShell } from "../shell/AppShell";
import { ProjectsScreen, type AssignableAccount } from "./ProjectsScreen";

const POLL_INTERVAL_MS = 5000;

/**
 * Polls the project registry over the selected-daemon `client` (list/create), and wires the
 * add-to-host RPC over a client addressed to the **chosen target host** (`clientForHost`) so the
 * request reaches that daemon directly rather than double-hopping through the selected daemon.
 * Every call site skips when its client is `null` (no daemon selected / no wire reaches it) rather
 * than throwing or faking success; see `useDaemonClient`'s contract.
 */
function useProjectsRpc(
  client: Client<typeof ProjectService> | null,
  clientForHost: (instanceId: string) => Client<typeof ProjectService> | null,
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

  // Unlike the setters above, this one lets a refusal reach the caller: the Home agent picker shows
  // the daemon's reason on its row.
  const setProjectHomeAgent = useCallback(
    (input: { projectId: string; homeAgent: string; daemonInstanceId: string }): Promise<void> => {
      if (!client) {
        return Promise.reject(new Error("no daemon selected to store the home agent"));
      }
      return client
        .setProjectHomeAgent({ sessionToken, ...input })
        .then(() => loadProjects());
    },
    [client, sessionToken, loadProjects],
  );

  const loadProjectBranches = useCallback(
    async (input: { projectId: string; daemonInstanceId: string }): Promise<{ branches: string[]; defaultRemote: string }> => {
      if (!client) return { branches: [], defaultRemote: "" };
      const res = await client.listProjectBranches({ sessionToken, ...input });
      return { branches: res.branches, defaultRemote: res.defaultRemote };
    },
    [client, sessionToken],
  );

  const setProjectAccounts = useCallback(
    (input: {
      projectId: string;
      accounts: { provider: string; accountId: string }[];
      localGh: boolean;
      daemonInstanceId: string;
    }) => {
      if (!client) return;
      client
        .setProjectAccounts({ sessionToken, ...input })
        .then(() => loadProjects())
        .catch(() => {});
    },
    [client, sessionToken, loadProjects],
  );

  return {
    projects,
    createProject,
    addProjectToHost,
    setDefaultBranch,
    loadProjectBranches,
    setProjectAccounts,
    setProjectHomeAgent,
  };
}

/**
 * The agents the catalog lists, from `CatalogService.ListAgents` — what a project's home agent is
 * chosen from. A failed load leaves the list empty, as the account list does.
 */
function useListedAgents(): AgentInfo[] {
  const client = useDaemonClient(CatalogService);
  const [agents, setAgents] = useState<AgentInfo[]>([]);

  useEffect(() => {
    if (!client) return;
    let current = true;
    client
      .listAgents({})
      .then((res) => {
        if (current) setAgents(res.agents);
      })
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [client]);

  return agents;
}

/**
 * The accounts the caller's vault holds, flattened from `ListAccounts`' provider groups. A locked,
 * absent or unreadable vault yields no assignable accounts; the Accounts screen is where those
 * states are explained.
 */
function useAssignableAccounts(sessionToken: string): AssignableAccount[] {
  const client = useDaemonClient(AccountsService);
  const [accounts, setAccounts] = useState<AssignableAccount[]>([]);

  useEffect(() => {
    if (!client) return;
    let current = true;
    client
      .listAccounts({ sessionToken })
      .then((res) => {
        if (!current) return;
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
      .catch(() => {});
    return () => {
      current = false;
    };
  }, [client, sessionToken]);

  return accounts;
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
  const accounts = useAssignableAccounts(sessionToken ?? "");
  const agents = useListedAgents();
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
    createProject,
    addProjectToHost,
    setDefaultBranch,
    loadProjectBranches,
    setProjectAccounts,
    setProjectHomeAgent,
  } = useProjectsRpc(client, clientForHost, sessionToken ?? "");

  return (
    <AppShell title="Projects" onNavigate={onNavigate} variant="scroll">
      <ProjectsScreen
        projects={projects}
        daemons={daemons}
        onCreateProject={createProject}
        onAddProjectToHost={addProjectToHost}
        onSetDefaultBranch={setDefaultBranch}
        loadProjectBranches={loadProjectBranches}
        accounts={accounts}
        onSetProjectAccounts={setProjectAccounts}
        loadHostGithubCli={loadHostGithubCli}
        agents={agents}
        onSetHomeAgent={setProjectHomeAgent}
      />
    </AppShell>
  );
}
