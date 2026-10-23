/**
 * Fixtures for specs that mount the Projects screen with its data container (`ProjectsAppPage`):
 * the hosts it can choose between, a project row, the Projects RPCs answering normally, and the
 * mount wrapper with the providers the page reads.
 */

import React from "react";
import { Room } from "livekit-client";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../../src/components/projects/ProjectsAppPage";
import { ProjectService, type ProjectEntry } from "../../../src/gen/project_pb";
import { AccountsService } from "../../../src/gen/accounts_pb";
import type { DaemonHost } from "../../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../../src/hooks/authProvider";

/** The host that served the page, and the one selected. */
export const PROJECTS_LOCAL_HOST = "workstation-1";
/** Another listed host, for the add-to-host flows. */
export const PROJECTS_REMOTE_HOST = "server-2";

/** Both hosts, each advertising itself the way a daemon does. */
export const PROJECTS_DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: PROJECTS_LOCAL_HOST, label: "workstation-1 (this daemon)" },
  { instanceId: PROJECTS_REMOTE_HOST, label: "server-2" },
];

export function aProject(overrides: Partial<ProjectEntry> = {}): ProjectEntry {
  return {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: PROJECTS_LOCAL_HOST,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    ...overrides,
  } as ProjectEntry;
}

/**
 * The Projects RPCs answering normally — one project, its branches, a create that adds a row, and an
 * empty vault — with `configure` applied last: a method registered twice is answered by its last
 * registration, so a failure set there replaces the default.
 */
export function aProjectsBackend(
  configure: (b: InMemoryRpcBackend) => InMemoryRpcBackend = (b) => b,
): InMemoryRpcBackend {
  const projects = [aProject()];
  const backend = anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(ProjectService.method.createProject, (req) => {
      const project = aProject({ projectId: "proj-new", name: req.name });
      projects.push(project);
      return { project };
    })
    .onUnary(AccountsService.method.listAccounts, () => ({ providers: [] }) as never);
  return configure(backend);
}

/** The Projects page under its providers. `room: null` mounts no LiveKit wire, so no client reaches any host. */
export function projectsAppPage(room: Room | null = new Room()) {
  return (
    <AuthProvider>
      <SelectedDaemonProvider
        room={room}
        daemons={PROJECTS_DAEMON_HOSTS}
        servingInstanceId={PROJECTS_LOCAL_HOST}
      >
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}
