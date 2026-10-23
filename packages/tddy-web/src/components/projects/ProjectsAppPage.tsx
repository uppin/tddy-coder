import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { type ProjectEntry } from "../../gen/project_pb";
import { ProjectService } from "../../gen/project_pb";
import { AccountsService } from "../../gen/accounts_pb";
import { CatalogService, type AgentInfo } from "../../gen/catalog_pb";
import { HostService } from "../../gen/host_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { useHostConnector } from "../../rpc/connections/registry";
import { useDaemonClient, useDaemons, useHostLabel, useSelectedDaemon } from "../../rpc/selectedDaemon";
import {
  hostNotConnected,
  hostNotConnectedText,
  rpcFailureText,
  unservedService,
} from "../../rpc/rpcFailure";
import { AppShell } from "../shell/AppShell";
import { ProjectsScreen, type AssignableAccount } from "./ProjectsScreen";

/** How often the Projects screen re-reads the registry. */
export const POLL_INTERVAL_MS = 5000;

interface ProjectsRpcOptions {
  /** The selected daemon's `ProjectService` client; `null` when no wire reaches it. */
  client: Client<typeof ProjectService> | null;
  /** A client addressed to a chosen target host, resolved when the operator submits. */
  clientForHost: (instanceId: string) => Client<typeof ProjectService> | null;
  sessionToken: string;
  /** A host's display name — what a "not connected" error names (see `useHostLabel`). */
  hostLabel: (instanceId: string) => string;
  selectedInstanceId: string | null;
}

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
function useProjectsRpc({
  client,
  clientForHost,
  sessionToken,
  hostLabel,
  selectedInstanceId,
}: ProjectsRpcOptions) {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);

  const connected = useCallback((): Client<typeof ProjectService> => {
    if (!client) throw hostNotConnected(hostLabel(selectedInstanceId ?? ""));
    return client;
  }, [client, hostLabel, selectedInstanceId]);

  const loadProjects = useCallback(() => {
    if (!client) {
      setLoadError(hostNotConnectedText(hostLabel(selectedInstanceId ?? "")));
      return;
    }
    client
      .listProjects({ sessionToken })
      .then((res) => {
        setProjects(res.projects);
        setLoadError(null);
      })
      .catch((e: unknown) => setLoadError(rpcFailureText(e)));
  }, [client, sessionToken, hostLabel, selectedInstanceId]);

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
      if (!target) throw hostNotConnected(hostLabel(input.daemonInstanceId));
      await target.addProjectToHost({ sessionToken, mainBranchRef: "", ...input });
      loadProjects();
    },
    [clientForHost, hostLabel, sessionToken, loadProjects],
  );

  const setDefaultBranch = useCallback(
    async (input: { projectId: string; mainBranchRef: string; daemonInstanceId: string }) => {
      await connected().setProjectDefaultBranch({ sessionToken, ...input });
      loadProjects();
    },
    [connected, sessionToken, loadProjects],
  );

  // Unlike the setters above, this one lets a refusal reach the caller: the Home agent picker shows
  // the daemon's reason on its row.
  const setProjectHomeAgent = useCallback(
    async (input: { projectId: string; homeAgent: string; daemonInstanceId: string }) => {
      await connected().setProjectHomeAgent({ sessionToken, ...input });
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
 * Why the vault's accounts could not be read. A daemon with no `auth_storage` registers no
 * AccountsService at all, so its router's "Unknown service" is said as the config that is missing.
 */
function accountsFailureText(error: unknown): string {
  if (unservedService(error) === AccountsService.typeName) {
    return "Accounts are not enabled on this host — its daemon config sets no auth_storage, so there is no credential vault to assign accounts from.";
  }
  return rpcFailureText(error);
}

/**
 * The accounts the caller's vault holds, flattened from `ListAccounts`' provider groups. A locked or
 * absent vault yields no assignable accounts — the Accounts screen is where those states are
 * explained — but a failed read is returned as `error`, so empty pickers say why. No client is not
 * reported here: the project list already says the host is not connected. What one client read is
 * dropped when the client changes or goes away: it is another host's answer, or a stale one.
 */
function useAssignableAccounts(sessionToken: string): { accounts: AssignableAccount[]; error: string | null } {
  const client = useDaemonClient(AccountsService);
  const [accounts, setAccounts] = useState<AssignableAccount[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setAccounts([]);
    setError(null);
  }, [client]);

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
        if (current) setError(accountsFailureText(e));
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
  const { selectedInstanceId } = useSelectedDaemon();
  // A host's display name — what a "not connected" error names. Stable across directory updates,
  // so the project poll is not restarted on every participant event.
  const hostLabel = useHostLabel();
  const { accounts, error: accountsLoadError } = useAssignableAccounts(sessionToken ?? "");
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
    loadError,
    createProject,
    addProjectToHost,
    setDefaultBranch,
    loadProjectBranches,
    setProjectAccounts,
    setProjectHomeAgent,
  } = useProjectsRpc({
    client,
    clientForHost,
    sessionToken: sessionToken ?? "",
    hostLabel,
    selectedInstanceId,
  });

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
        agents={agents}
        onSetHomeAgent={setProjectHomeAgent}
      />
    </AppShell>
  );
}
