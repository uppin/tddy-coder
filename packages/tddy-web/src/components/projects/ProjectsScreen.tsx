import { useEffect, useMemo, useState } from "react";
import type { ProjectEntry } from "../../gen/session_pb";
import type { DaemonHost } from "../../lib/participantRole";

/**
 * Presentational Projects screen: lists projects grouped by logical `projectId` (a project may
 * live on multiple hosts) and exposes create-project + add-to-host actions. All RPC wiring lives
 * in the container (`ProjectsAppPage`); this component is pure props + local UI state.
 */

export interface ProjectsScreenProps {
  projects: ProjectEntry[];
  daemons: DaemonHost[];
  onCreateProject: (input: { name: string; gitUrl: string; userRelativePath: string }) => void;
  onAddProjectToHost: (input: {
    projectId: string;
    name: string;
    gitUrl: string;
    daemonInstanceId: string;
    userRelativePath: string;
  }) => void;
  onSetDefaultBranch: (input: {
    projectId: string;
    mainBranchRef: string;
    daemonInstanceId: string;
  }) => void;
  loadProjectBranches: (input: {
    projectId: string;
    daemonInstanceId: string;
  }) => Promise<{ branches: string[]; defaultRemote: string }>;
  /**
   * Every account the caller's vault holds, in the order the daemon grouped them. The assignment
   * rows a project card offers are exactly the providers present here — a provider with no linked
   * account has nothing to assign, so it gets no row.
   */
  accounts: AssignableAccount[];
  /**
   * Replace a project's whole assignment set. Not a merge: the set sent is the set stored, which is
   * how an assignment is cleared (send the remaining ones without it).
   */
  onSetProjectAccounts: (input: {
    projectId: string;
    accounts: { provider: string; accountId: string }[];
    daemonInstanceId: string;
  }) => void;
}

/**
 * One account a project can be assigned, flattened from `ListAccountsResponse`. An account id is
 * unique only **within** its provider, so both halves travel together everywhere.
 */
export interface AssignableAccount {
  provider: string;
  accountId: string;
  label: string;
}

interface ProjectGroup {
  projectId: string;
  name: string;
  gitUrl: string;
  mainBranchRef: string;
  accounts: { provider: string; accountId: string }[];
  hosts: { daemonInstanceId: string; mainRepoPath: string }[];
}

/** Group registry rows by `projectId`, preserving first-seen order for both projects and hosts. */
function groupByProject(projects: ProjectEntry[]): ProjectGroup[] {
  const groups: ProjectGroup[] = [];
  const byId = new Map<string, ProjectGroup>();
  for (const p of projects) {
    let group = byId.get(p.projectId);
    if (!group) {
      group = {
        projectId: p.projectId,
        name: p.name,
        gitUrl: p.gitUrl,
        mainBranchRef: p.mainBranchRef,
        accounts: p.accounts.map((a) => ({ provider: a.provider, accountId: a.accountId })),
        hosts: [],
      };
      byId.set(p.projectId, group);
      groups.push(group);
    }
    group.hosts.push({ daemonInstanceId: p.daemonInstanceId, mainRepoPath: p.mainRepoPath });
  }
  return groups;
}

interface AssignmentRow {
  provider: string;
  /** "" when the project holds no assignment at this provider. */
  assignedAccountId: string;
  /** The assigned account is not among this host's vault accounts for the provider. */
  assignedUnavailable: boolean;
  options: AssignableAccount[];
}

/**
 * One row per provider the vault holds an account at, in vault order, plus a row for any provider
 * the project is assigned at whose account this host does not know — that assignment is still real
 * and must be visible, as "unavailable", never silently shown as unassigned.
 */
function assignmentRowsFor(
  assigned: { provider: string; accountId: string }[],
  vault: AssignableAccount[],
): AssignmentRow[] {
  const providers = [...new Set([...vault.map((a) => a.provider), ...assigned.map((a) => a.provider)])];
  return providers.map((provider) => {
    const options = vault.filter((a) => a.provider === provider);
    const assignedAccountId = assigned.find((a) => a.provider === provider)?.accountId ?? "";
    return {
      provider,
      assignedAccountId,
      assignedUnavailable:
        assignedAccountId !== "" && !options.some((a) => a.accountId === assignedAccountId),
      options,
    };
  });
}

/** The whole assignment set with one provider's entry replaced, or removed when `accountId` is "". */
function withAssignment(
  assigned: { provider: string; accountId: string }[],
  provider: string,
  accountId: string,
): { provider: string; accountId: string }[] {
  const others = assigned.filter((a) => a.provider !== provider);
  if (accountId === "") return others;
  const existing = assigned.some((a) => a.provider === provider);
  return existing
    ? assigned.map((a) => (a.provider === provider ? { provider, accountId } : a))
    : [...others, { provider, accountId }];
}

/**
 * The branch to show selected when a project has no stored default: mirror the daemon's live
 * resolution order (`<remote>/master`, then `<remote>/main`), else the first branch. The remote is
 * the project's resolved default (`defaultRemote` from `ListProjectBranchesResponse`); `origin` is
 * only the last-resort fallback when the daemon could not detect one.
 */
function defaultSelectedBranch(branches: string[], remote: string): string {
  const r = remote || "origin";
  if (branches.includes(`${r}/master`)) return `${r}/master`;
  if (branches.includes(`${r}/main`)) return `${r}/main`;
  return branches[0] ?? "";
}

export function ProjectsScreen({
  projects,
  daemons,
  onCreateProject,
  onAddProjectToHost,
  onSetDefaultBranch,
  loadProjectBranches,
  accounts,
  onSetProjectAccounts,
}: ProjectsScreenProps) {
  const groups = useMemo(() => groupByProject(projects), [projects]);

  const [createOpen, setCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [newGitUrl, setNewGitUrl] = useState("");
  const [newUserRelativePath, setNewUserRelativePath] = useState("");

  const submitCreate = () => {
    onCreateProject({
      name: newName.trim(),
      gitUrl: newGitUrl.trim(),
      userRelativePath: newUserRelativePath.trim(),
    });
    setNewName("");
    setNewGitUrl("");
    setNewUserRelativePath("");
    setCreateOpen(false);
  };

  return (
    <div data-testid="projects-screen">
      <div className="mb-6">
        <button
          type="button"
          data-testid="projects-create-project-toggle"
          className="rounded-md border border-border px-3 py-2 text-sm font-medium"
          onClick={() => setCreateOpen((o) => !o)}
        >
          Create project
        </button>
        {createOpen ? (
          <div
            data-testid="projects-create-project-form"
            className="mt-3 flex flex-col gap-2 rounded-md border border-border p-3"
          >
            <input
              data-testid="projects-new-project-name"
              placeholder="Project name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              className="rounded border border-border px-2 py-1"
            />
            <input
              data-testid="projects-new-project-git-url"
              placeholder="Git URL"
              value={newGitUrl}
              onChange={(e) => setNewGitUrl(e.target.value)}
              className="rounded border border-border px-2 py-1"
            />
            <input
              data-testid="projects-new-project-user-relative-path"
              placeholder="Path relative to home (optional)"
              value={newUserRelativePath}
              onChange={(e) => setNewUserRelativePath(e.target.value)}
              className="rounded border border-border px-2 py-1"
            />
            <button
              type="button"
              data-testid="projects-create-project-submit"
              className="self-start rounded-md border border-border px-3 py-2 text-sm font-medium"
              onClick={submitCreate}
            >
              Create
            </button>
          </div>
        ) : null}
      </div>

      <div data-testid="projects-list" className="flex flex-col gap-4">
        {groups.map((group) => (
          <ProjectCard
            key={group.projectId}
            group={group}
            daemons={daemons}
            onAddProjectToHost={onAddProjectToHost}
            onSetDefaultBranch={onSetDefaultBranch}
            loadProjectBranches={loadProjectBranches}
            accounts={accounts}
            onSetProjectAccounts={onSetProjectAccounts}
          />
        ))}
      </div>
    </div>
  );
}

function ProjectCard({
  group,
  daemons,
  onAddProjectToHost,
  onSetDefaultBranch,
  loadProjectBranches,
  accounts,
  onSetProjectAccounts,
}: {
  group: ProjectGroup;
  daemons: DaemonHost[];
  onAddProjectToHost: ProjectsScreenProps["onAddProjectToHost"];
  onSetDefaultBranch: ProjectsScreenProps["onSetDefaultBranch"];
  loadProjectBranches: ProjectsScreenProps["loadProjectBranches"];
  accounts: ProjectsScreenProps["accounts"];
  onSetProjectAccounts: ProjectsScreenProps["onSetProjectAccounts"];
}) {
  const hostingIds = useMemo(
    () => new Set(group.hosts.map((h) => h.daemonInstanceId)),
    [group.hosts],
  );

  // The default branch is a property of the logical project; load the branch list from — and
  // address the set-default RPC to — the project's first hosting daemon.
  const primaryHost = group.hosts[0]?.daemonInstanceId ?? "";
  const [branches, setBranches] = useState<string[]>([]);
  const [defaultRemote, setDefaultRemote] = useState<string>("");
  useEffect(() => {
    let cancelled = false;
    loadProjectBranches({ projectId: group.projectId, daemonInstanceId: primaryHost })
      .then((loaded) => {
        if (!cancelled) {
          setBranches(loaded.branches);
          setDefaultRemote(loaded.defaultRemote);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [loadProjectBranches, group.projectId, primaryHost]);

  const selectedDefaultBranch = group.mainBranchRef || defaultSelectedBranch(branches, defaultRemote);
  const targetDaemons = useMemo(
    () => daemons.filter((d) => !hostingIds.has(d.instanceId)),
    [daemons, hostingIds],
  );
  const baseLocationByHost = useMemo(
    () => new Map(daemons.map((d) => [d.instanceId, d.reposBasePath])),
    [daemons],
  );

  const assignmentRows = useMemo(
    () => assignmentRowsFor(group.accounts, accounts),
    [group.accounts, accounts],
  );
  const assignAccount = (provider: string, accountId: string) =>
    onSetProjectAccounts({
      projectId: group.projectId,
      accounts: withAssignment(group.accounts, provider, accountId),
      daemonInstanceId: primaryHost,
    });

  const [addOpen, setAddOpen] = useState(false);
  const [selectedHost, setSelectedHost] = useState("");
  const [userRelativePath, setUserRelativePath] = useState("");

  // Default the selection to the first available target once the control opens.
  const effectiveSelection =
    selectedHost || (targetDaemons.length > 0 ? targetDaemons[0].instanceId : "");

  return (
    <div
      data-testid={`project-card-${group.projectId}`}
      className="rounded-md border border-border p-4"
    >
      <div className="mb-2 font-semibold">{group.name}</div>
      <div className="mb-3 text-sm text-muted-foreground">{group.gitUrl}</div>

      <div className="mb-3 flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">Default branch</span>
        <select
          data-testid={`project-default-branch-select-${group.projectId}`}
          value={selectedDefaultBranch}
          onChange={(e) =>
            onSetDefaultBranch({
              projectId: group.projectId,
              mainBranchRef: e.target.value,
              daemonInstanceId: primaryHost,
            })
          }
          className="rounded border border-border px-2 py-1"
        >
          {branches.map((branch) => (
            <option key={branch} value={branch}>
              {branch}
            </option>
          ))}
        </select>
      </div>

      <div className="mb-3 flex flex-col gap-1">
        {assignmentRows.map((row) => (
          <div
            key={row.provider}
            data-testid={`project-account-row-${group.projectId}-${row.provider}`}
            className="flex items-center gap-2 text-sm"
          >
            <span className="text-muted-foreground">{row.provider}</span>
            <select
              data-testid={`project-account-select-${group.projectId}-${row.provider}`}
              value={row.assignedAccountId}
              onChange={(e) => assignAccount(row.provider, e.target.value)}
              className="rounded border border-border px-2 py-1"
            >
              <option value="">No account assigned</option>
              {row.assignedUnavailable ? (
                <option value={row.assignedAccountId}>
                  Unavailable on this host ({row.assignedAccountId})
                </option>
              ) : null}
              {row.options.map((option) => (
                <option key={option.accountId} value={option.accountId}>
                  {option.label}
                </option>
              ))}
            </select>
            {row.assignedUnavailable ? (
              <span
                data-testid={`project-account-unavailable-${group.projectId}-${row.provider}`}
                className="text-destructive"
              >
                The assigned account is not in this host's vault
              </span>
            ) : null}
          </div>
        ))}
      </div>

      <div className="flex flex-col gap-1">
        {group.hosts.map((host) => {
          const baseLocation = baseLocationByHost.get(host.daemonInstanceId);
          return (
            <div
              key={host.daemonInstanceId}
              data-testid={`project-host-row-${group.projectId}-${host.daemonInstanceId}`}
              className="flex items-center gap-2 text-sm"
            >
              <span className="font-medium">{host.daemonInstanceId}</span>
              <span className="text-muted-foreground">{host.mainRepoPath}</span>
              {baseLocation ? (
                <span
                  data-testid={`project-host-base-location-${host.daemonInstanceId}`}
                  className="text-muted-foreground"
                >
                  base: {baseLocation}
                </span>
              ) : null}
            </div>
          );
        })}
      </div>

      <div className="mt-3">
        <button
          type="button"
          data-testid={`project-add-to-host-toggle-${group.projectId}`}
          className="rounded-md border border-border px-3 py-1 text-sm"
          disabled={targetDaemons.length === 0}
          onClick={() => setAddOpen((o) => !o)}
        >
          Add to host
        </button>
        {addOpen ? (
          <div className="mt-2 flex items-center gap-2">
            <select
              data-testid={`project-add-to-host-select-${group.projectId}`}
              value={effectiveSelection}
              onChange={(e) => setSelectedHost(e.target.value)}
              className="rounded border border-border px-2 py-1"
            >
              {targetDaemons.map((d) => (
                <option key={d.instanceId} value={d.instanceId}>
                  {d.label}
                </option>
              ))}
            </select>
            <input
              data-testid={`project-add-to-host-user-relative-path-${group.projectId}`}
              placeholder={
                baseLocationByHost.get(effectiveSelection)
                  ? `Path relative to home (default ${baseLocationByHost.get(effectiveSelection)})`
                  : "Path relative to home (optional)"
              }
              value={userRelativePath}
              onChange={(e) => setUserRelativePath(e.target.value)}
              className="rounded border border-border px-2 py-1"
            />
            <button
              type="button"
              data-testid={`project-add-to-host-submit-${group.projectId}`}
              className="rounded-md border border-border px-3 py-1 text-sm"
              onClick={() =>
                onAddProjectToHost({
                  projectId: group.projectId,
                  name: group.name,
                  gitUrl: group.gitUrl,
                  daemonInstanceId: effectiveSelection,
                  userRelativePath: userRelativePath.trim(),
                })
              }
            >
              Add
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
