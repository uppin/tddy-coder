import { useEffect, useMemo, useState } from "react";
import type { DaemonHost } from "../../lib/participantRole";
import { useActionFailure } from "../../hooks/useActionFailure";
import { InlineError } from "../ui/inline-error";
import { assignmentRowsFor, withAssignment } from "./ProjectAccountAssignment";
import {
  githubHostnameOf,
  HostGithubState,
  LocalGhToggle,
  type LoadHostGithubCli,
} from "./ProjectLocalGh";
import { ProjectHomeAgent } from "./ProjectHomeAgent";
import type { ProjectsScreenProps } from "./ProjectsScreen";

/**
 * One logical project on the Projects screen: its default branch, account assignment, Local gh, home
 * agent, the hosts it lives on, and add-to-host. Its branch load and every action taken on it share
 * one failure slot, rendered on the card.
 */

export interface ProjectGroup {
  projectId: string;
  name: string;
  gitUrl: string;
  mainBranchRef: string;
  accounts: { provider: string; accountId: string }[];
  localGh: boolean;
  homeAgent: string;
  hosts: { daemonInstanceId: string; mainRepoPath: string }[];
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

export function ProjectCard({
  group,
  daemons,
  onAddProjectToHost,
  onSetDefaultBranch,
  loadProjectBranches,
  accounts,
  onSetProjectAccounts,
  loadHostGithubCli,
  agents,
  onSetHomeAgent,
  focusHomeAgent = false,
}: {
  group: ProjectGroup;
  daemons: DaemonHost[];
  onAddProjectToHost: ProjectsScreenProps["onAddProjectToHost"];
  onSetDefaultBranch: ProjectsScreenProps["onSetDefaultBranch"];
  loadProjectBranches: ProjectsScreenProps["loadProjectBranches"];
  accounts: ProjectsScreenProps["accounts"];
  onSetProjectAccounts: ProjectsScreenProps["onSetProjectAccounts"];
  loadHostGithubCli: LoadHostGithubCli;
  agents: { id: string; label: string }[];
  onSetHomeAgent: ProjectsScreenProps["onSetHomeAgent"];
  /** Focus this card's home-agent picker and scroll it into view. */
  focusHomeAgent?: boolean;
}) {
  const hostingIds = useMemo(
    () => new Set(group.hosts.map((h) => h.daemonInstanceId)),
    [group.hosts],
  );

  // The default branch is a property of the logical project; load the branch list from — and
  // address the set-default RPC to — the project's first hosting daemon.
  const primaryHost = group.hosts[0]?.daemonInstanceId ?? "";
  // The card's one failure slot: its branch load and every action taken on it.
  const failure = useActionFailure();
  const { report } = failure;
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
      .catch((e: unknown) => {
        if (!cancelled) report(e);
      });
    return () => {
      cancelled = true;
    };
  }, [loadProjectBranches, group.projectId, primaryHost, report]);

  const selectedDefaultBranch = group.mainBranchRef || defaultSelectedBranch(branches, defaultRemote);
  const targetDaemons = useMemo(
    () => daemons.filter((d) => !hostingIds.has(d.instanceId)),
    [daemons, hostingIds],
  );
  const baseLocationByHost = useMemo(
    () => new Map(daemons.map((d) => [d.instanceId, d.reposBasePath])),
    [daemons],
  );

  // A project has one GitHub source: while Local gh is on, no GitHub account can be assigned.
  const assignmentRows = useMemo(
    () =>
      assignmentRowsFor(group.accounts, accounts).filter(
        (row) => !(group.localGh && row.provider === "github"),
      ),
    [group.accounts, group.localGh, accounts],
  );
  const assignAccount = (provider: string, accountId: string) =>
    void failure.run(() =>
      onSetProjectAccounts({
        projectId: group.projectId,
        accounts: withAssignment(group.accounts, provider, accountId),
        localGh: group.localGh,
        daemonInstanceId: primaryHost,
      }),
    );
  const setLocalGh = (localGh: boolean) =>
    void failure.run(() =>
      onSetProjectAccounts({
        projectId: group.projectId,
        accounts: localGh ? withAssignment(group.accounts, "github", "") : group.accounts,
        localGh,
        daemonInstanceId: primaryHost,
      }),
    );
  const githubHostname = githubHostnameOf(group.gitUrl);

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
      <InlineError testId={`project-card-error-${group.projectId}`} message={failure.error} className="mb-3" />

      <div className="mb-3 flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">Default branch</span>
        <select
          data-testid={`project-default-branch-select-${group.projectId}`}
          value={selectedDefaultBranch}
          onChange={(e) => {
            const mainBranchRef = e.target.value;
            void failure.run(() =>
              onSetDefaultBranch({
                projectId: group.projectId,
                mainBranchRef,
                daemonInstanceId: primaryHost,
              }),
            );
          }}
          className="rounded border border-border px-2 py-1"
        >
          {branches.map((branch) => (
            <option key={branch} value={branch}>
              {branch}
            </option>
          ))}
        </select>
      </div>

      {onSetHomeAgent ? (
        <ProjectHomeAgent
          project={{ projectId: group.projectId, homeAgent: group.homeAgent }}
          agents={agents}
          focused={focusHomeAgent}
          onChange={(agentId) =>
            onSetHomeAgent({
              projectId: group.projectId,
              homeAgent: agentId,
              daemonInstanceId: primaryHost,
            })
          }
        />
      ) : null}

      <LocalGhToggle projectId={group.projectId} checked={group.localGh} onChange={setLocalGh} />

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
              {group.localGh ? (
                <HostGithubState
                  projectId={group.projectId}
                  daemonInstanceId={host.daemonInstanceId}
                  githubHostname={githubHostname}
                  loadHostGithubCli={loadHostGithubCli}
                />
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
              disabled={failure.pending}
              onClick={() =>
                void failure.run(() =>
                  onAddProjectToHost({
                    projectId: group.projectId,
                    name: group.name,
                    gitUrl: group.gitUrl,
                    daemonInstanceId: effectiveSelection,
                    userRelativePath: userRelativePath.trim(),
                  }),
                )
              }
            >
              {failure.pending ? "Adding…" : "Add"}
            </button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
