import { useMemo, useState } from "react";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import type { DaemonHost } from "tddy-client-web";
import { useActionFailure } from "tddy-client-web";
import { InlineError } from "tddy-ui-web";
import type { AssignableAccount } from "./ProjectAccountAssignment";
import type { LoadHostGithubCli } from "./ProjectLocalGh";
import { ProjectCard, type ProjectGroup } from "./ProjectCard";
import { DeletedProjectCard } from "./DeletedProjectCard";

export type { AssignableAccount };

/**
 * Presentational Projects screen: lists projects grouped by logical `projectId` (a project may
 * live on multiple hosts) and exposes create-project + add-to-host actions. All RPC wiring lives
 * in the container (`ProjectsContent`); this component is pure props + local UI state.
 *
 * Every action prop returns a promise that rejects with why it failed, and the reason is shown
 * where the action was taken: the create form, or the project's card. Nothing is swallowed.
 *
 * A soft-deleted project (listed only while "Show deleted projects" is ticked) is a
 * {@link DeletedProjectCard} with **Restore**, not an ordinary card.
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
  /** Whether soft-deleted projects are listed (the "Show deleted projects" checkbox). */
  showDeleted?: boolean;
  /** Tick or untick "Show deleted projects". Absent: the checkbox is not offered. */
  onShowDeletedChange?: (showDeleted: boolean) => void;
  /** Restore a soft-deleted project (`RestoreProject`). Absent: deleted cards offer no Restore. */
  onRestoreProject?: (projectId: string) => Promise<void>;
  /**
   * Save a project's name and git URL (`UpdateProject`, which the daemon fans out to every host
   * owning the project). Absent: no Edit is offered on the cards.
   */
  onUpdateProject?: (input: {
    projectId: string;
    name: string;
    gitUrl: string;
  }) => Promise<void>;
  /**
   * Move one host's checkout path (`UpdateProject` with `local_only`, addressed at
   * `daemonInstanceId`). Absent: the Edit form offers no path fields.
   */
  onMoveProjectCheckout?: (input: {
    projectId: string;
    name: string;
    mainRepoPath: string;
    daemonInstanceId: string;
  }) => Promise<void>;
  /**
   * Soft-delete a project (`DeleteProject`, fanned out). Absent: no Delete is offered on the cards.
   */
  onDeleteProject?: (projectId: string) => Promise<void>;
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
        hosts: [],
        deletedAt: "",
        deletedBy: "",
      };
      byId.set(p.projectId, group);
      groups.push(group);
    }
    // The daemon gives every row of a project the same lifecycle; any deleted row marks the group.
    if (!group.deletedAt && p.deletedAt) {
      group.deletedAt = p.deletedAt;
      group.deletedBy = p.deletedBy;
    }
    group.hosts.push({
      daemonInstanceId: p.daemonInstanceId,
      mainRepoPath: p.mainRepoPath,
      homeSessionId: p.homeSessionId,
    });
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
  showDeleted = false,
  onShowDeletedChange,
  onRestoreProject,
  onUpdateProject,
  onMoveProjectCheckout,
  onDeleteProject,
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

      {onShowDeletedChange ? (
        <label className="mb-4 flex items-center gap-2 text-sm text-muted-foreground">
          <input
            type="checkbox"
            data-testid="projects-show-deleted"
            checked={showDeleted}
            onChange={(e) => onShowDeletedChange(e.target.checked)}
          />
          Show deleted projects
        </label>
      ) : null}

      <InlineError testId="projects-load-error" message={loadError} className="mb-4" />
      <InlineError testId="projects-accounts-load-error" message={accountsLoadError} className="mb-4" />

      <div data-testid="projects-list" className="flex flex-col gap-4">
        {groups.map((group) =>
          group.deletedAt ? (
            <DeletedProjectCard key={group.projectId} group={group} onRestoreProject={onRestoreProject} />
          ) : (
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
              onUpdateProject={onUpdateProject}
              onMoveProjectCheckout={onMoveProjectCheckout}
              onDeleteProject={onDeleteProject}
            />
          ),
        )}
      </div>
    </div>
  );
}
