import { useMemo, useState } from "react";
import type { ProjectEntry } from "../../gen/project_pb";
import type { DaemonHost } from "../../lib/participantRole";
import { useActionFailure } from "../../hooks/useActionFailure";
import { InlineError } from "../ui/inline-error";
import type { AssignableAccount } from "./ProjectAccountAssignment";
import type { LoadHostGithubCli } from "./ProjectLocalGh";
import { ProjectCard, type ProjectGroup } from "./ProjectCard";

export type { AssignableAccount };

/**
 * Presentational Projects screen: lists projects grouped by logical `projectId` (a project may
 * live on multiple hosts) and exposes create-project + add-to-host actions. All RPC wiring lives
 * in the container (`ProjectsAppPage`); this component is pure props + local UI state.
 *
 * Every action prop returns a promise that rejects with why it failed, and the reason is shown
 * where the action was taken: the create form, or the project's card. Nothing is swallowed.
 */

export interface ProjectsScreenProps {
  projects: ProjectEntry[];
  /** Why the project list could not be loaded; the last good list stays shown beside it. */
  loadError?: string | null;
  /** Why the vault's accounts could not be read, which leaves the assignment pickers empty. */
  accountsLoadError?: string | null;
  daemons: DaemonHost[];
  onCreateProject: (input: { name: string; gitUrl: string; userRelativePath: string }) => Promise<void>;
  onAddProjectToHost: (input: {
    projectId: string;
    name: string;
    gitUrl: string;
    daemonInstanceId: string;
    userRelativePath: string;
  }) => Promise<void>;
  onSetDefaultBranch: (input: {
    projectId: string;
    mainBranchRef: string;
    daemonInstanceId: string;
  }) => Promise<void>;
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
    /** The project's whole Local gh setting, written with `accounts`; never true beside a github
     *  assignment. */
    localGh: boolean;
    daemonInstanceId: string;
  }) => Promise<void>;
  /**
   * What one host's `gh` says about the project's GitHub host (`GetHostTooling` with
   * `githubHostname`). Asked for each host of a Local gh project.
   */
  loadHostGithubCli: LoadHostGithubCli;
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
        localGh: p.localGh,
        homeSessionId: p.homeSessionId,
        hosts: [],
      };
      byId.set(p.projectId, group);
      groups.push(group);
    }
    group.hosts.push({ daemonInstanceId: p.daemonInstanceId, mainRepoPath: p.mainRepoPath });
  }
  return groups;
}


export function ProjectsScreen({
  projects,
  loadError,
  accountsLoadError,
  daemons,
  onCreateProject,
  onAddProjectToHost,
  onSetDefaultBranch,
  loadProjectBranches,
  accounts,
  onSetProjectAccounts,
  loadHostGithubCli,
}: ProjectsScreenProps) {
  const groups = useMemo(() => groupByProject(projects), [projects]);

  const [createOpen, setCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");
  const [newGitUrl, setNewGitUrl] = useState("");
  const [newUserRelativePath, setNewUserRelativePath] = useState("");

  const create = useActionFailure();

  // The form keeps what was typed until the project exists, so a failure can be corrected in place.
  const submitCreate = async () => {
    const created = await create.run(() =>
      onCreateProject({
        name: newName.trim(),
        gitUrl: newGitUrl.trim(),
        userRelativePath: newUserRelativePath.trim(),
      }),
    );
    if (!created) return;
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
              disabled={create.pending}
              onClick={() => void submitCreate()}
            >
              {create.pending ? "Creating…" : "Create"}
            </button>
            <InlineError testId="projects-create-project-error" message={create.error} />
          </div>
        ) : null}
      </div>

      <InlineError testId="projects-load-error" message={loadError} className="mb-4" />
      <InlineError testId="projects-accounts-load-error" message={accountsLoadError} className="mb-4" />

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
            loadHostGithubCli={loadHostGithubCli}
          />
        ))}
      </div>
    </div>
  );
}
