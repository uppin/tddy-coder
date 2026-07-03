import { useMemo, useState } from "react";
import type { ProjectEntry } from "../../gen/connection_pb";
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
  }) => void;
}

interface ProjectGroup {
  projectId: string;
  name: string;
  gitUrl: string;
  hosts: { daemonInstanceId: string; mainRepoPath: string }[];
}

/** Group registry rows by `projectId`, preserving first-seen order for both projects and hosts. */
function groupByProject(projects: ProjectEntry[]): ProjectGroup[] {
  const groups: ProjectGroup[] = [];
  const byId = new Map<string, ProjectGroup>();
  for (const p of projects) {
    let group = byId.get(p.projectId);
    if (!group) {
      group = { projectId: p.projectId, name: p.name, gitUrl: p.gitUrl, hosts: [] };
      byId.set(p.projectId, group);
      groups.push(group);
    }
    group.hosts.push({ daemonInstanceId: p.daemonInstanceId, mainRepoPath: p.mainRepoPath });
  }
  return groups;
}

export function ProjectsScreen({
  projects,
  daemons,
  onCreateProject,
  onAddProjectToHost,
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
}: {
  group: ProjectGroup;
  daemons: DaemonHost[];
  onAddProjectToHost: ProjectsScreenProps["onAddProjectToHost"];
}) {
  const hostingIds = useMemo(
    () => new Set(group.hosts.map((h) => h.daemonInstanceId)),
    [group.hosts],
  );
  const targetDaemons = useMemo(
    () => daemons.filter((d) => !hostingIds.has(d.instanceId)),
    [daemons, hostingIds],
  );

  const [addOpen, setAddOpen] = useState(false);
  const [selectedHost, setSelectedHost] = useState("");

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

      <div className="flex flex-col gap-1">
        {group.hosts.map((host) => (
          <div
            key={host.daemonInstanceId}
            data-testid={`project-host-row-${group.projectId}-${host.daemonInstanceId}`}
            className="flex items-center gap-2 text-sm"
          >
            <span className="font-medium">{host.daemonInstanceId}</span>
            <span className="text-muted-foreground">{host.mainRepoPath}</span>
          </div>
        ))}
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
