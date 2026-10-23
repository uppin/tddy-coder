/**
 * Acceptance tests: the Projects screen says why an action failed instead of doing nothing.
 *
 * Every Projects RPC used to end in `.catch(() => {})`, and every call site returned early when no
 * client could reach the host — so a create against an unreachable host showed neither an error
 * nor a project. Each failure now lands next to the action that caused it: the create form, the
 * project card, or the list itself. A list that failed to load is never shown as an empty one.
 */

import React from "react";
import { Room } from "livekit-client";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import { ProjectsScreen } from "../../src/components/projects/ProjectsScreen";
import { ProjectService, type ProjectEntry } from "../../src/gen/project_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

const LOCAL_HOST = "workstation-1";
const REMOTE_HOST = "server-2";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
  { instanceId: REMOTE_HOST, label: "server-2" },
];

function aProject(overrides: Partial<ProjectEntry> = {}): ProjectEntry {
  return {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: LOCAL_HOST,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    ...overrides,
  } as ProjectEntry;
}

const noBranches = () =>
  Promise.resolve<{ branches: string[]; defaultRemote: string }>({ branches: [], defaultRemote: "" });

/**
 * The Projects RPCs answering normally, with `configure` applied last: a method registered twice is
 * answered by its last registration, so a failure set there replaces the default.
 */
function aProjectsBackend(configure: (b: InMemoryRpcBackend) => InMemoryRpcBackend = (b) => b) {
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
    });
  return configure(backend);
}

/** `room: null` mounts no LiveKit wire at all, so no client reaches any host. */
function projectsAppPage(room: Room | null = new Room()) {
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={room} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

function aProjectsScreen(overrides: Partial<React.ComponentProps<typeof ProjectsScreen>> = {}) {
  return (
    <ProjectsScreen
      projects={[aProject()]}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub().resolves()}
      onAddProjectToHost={cy.stub().resolves()}
      onSetDefaultBranch={cy.stub().resolves()}
      loadProjectBranches={noBranches}
      accounts={[{ provider: "github", accountId: "acct-1", label: "octocat" }]}
      onSetProjectAccounts={cy.stub().resolves()}
      loadHostGithubCli={cy.stub()}
      {...overrides}
    />
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Create project
// ---------------------------------------------------------------------------

it("shows the daemon's reason when creating a project fails, and keeps what was typed", () => {
  // Given
  const backend = aProjectsBackend((b) =>
    b.failWith(ProjectService.method.createProject, Code.FailedPrecondition, "git clone failed: repository not found"),
  );
  mountWithRpc(projectsAppPage(), backend);

  // When
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({ name: "beta", gitUrl: "https://example.com/beta.git" });

  // Then
  projectsScreenPage.createProjectError().should("contain.text", "git clone failed: repository not found");
  projectsScreenPage.newProjectNameInput().should("have.value", "beta");
});

it("closes the create form only once the project was created", () => {
  // Given
  mountWithRpc(projectsAppPage(), aProjectsBackend());

  // When
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({ name: "beta", gitUrl: "https://example.com/beta.git" });

  // Then
  projectsScreenPage.card("proj-new").should("exist");
  projectsScreenPage.createProjectForm().should("not.exist");
  projectsScreenPage.createProjectError().should("not.exist");
});

it("disables the create button while the request is in flight", () => {
  // Given a create that has not answered yet
  let answer: (() => void) | undefined;
  const onCreateProject = cy.stub().callsFake(
    () => new Promise<void>((resolve) => {
      answer = resolve;
    }),
  );
  cy.mount(aProjectsScreen({ onCreateProject }));

  // When
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({ name: "beta", gitUrl: "https://example.com/beta.git" });

  // Then
  projectsScreenPage.createProjectSubmit().should("be.disabled");
  cy.then(() => answer?.());
  projectsScreenPage.createProjectForm().should("not.exist");
});

// ---------------------------------------------------------------------------
// No reachable host
// ---------------------------------------------------------------------------

// With no wire at all the layout's gate covers the screen (`HostConnectionGateAcceptance`) — the
// screen's own error below it remains, so the screen never claims an empty registry either way.
it("says the host is not connected instead of showing an empty list", () => {
  // Given a selected host that no wire reaches
  cy.mount(projectsAppPage(null));

  // Then
  projectsScreenPage.loadError().should("contain.text", "Not connected to workstation-1");
});

it("shows the transport's reason, naming the host, when the common room never connected", () => {
  // Given a wire over a room that is not connected — the LiveKit-server-down case
  cy.mount(projectsAppPage(new Room()));

  // When
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({ name: "beta", gitUrl: "https://example.com/beta.git" });

  // Then
  projectsScreenPage.loadError().should("contain.text", "workstation-1");
  projectsScreenPage.createProjectError().should("contain.text", "workstation-1");
});

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

it("shows why the project list could not be loaded", () => {
  // Given
  const backend = aProjectsBackend((b) =>
    b.failWith(ProjectService.method.listProjects, Code.Internal, "project registry is unreadable"),
  );

  // When
  mountWithRpc(projectsAppPage(), backend);

  // Then
  projectsScreenPage.loadError().should("contain.text", "project registry is unreadable");
});

it("clears the list error once a later poll succeeds", () => {
  // Given a list that fails once, then answers
  let calls = 0;
  const backend = anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => {
      calls += 1;
      if (calls === 1) throw new ConnectError("registry busy", Code.Unavailable);
      return { projects: [aProject()] };
    })
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "" }));
  cy.clock();
  mountWithRpc(projectsAppPage(), backend);
  projectsScreenPage.loadError().should("contain.text", "registry busy");

  // When the next poll runs
  cy.tick(5000);

  // Then
  projectsScreenPage.card("proj-alpha").should("exist");
  projectsScreenPage.loadError().should("not.exist");
});

// ---------------------------------------------------------------------------
// Project card actions
// ---------------------------------------------------------------------------

it("shows on the card why adding the project to another host failed", () => {
  // Given
  const onAddProjectToHost = cy.stub().rejects(new ConnectError("server-2 has no ssh key for example.com", Code.FailedPrecondition));
  cy.mount(aProjectsScreen({ onAddProjectToHost }));

  // When
  projectsScreenPage.openAddToHost("proj-alpha");
  projectsScreenPage.addProjectToHost("proj-alpha", REMOTE_HOST);

  // Then
  projectsScreenPage.cardError("proj-alpha").should("contain.text", "server-2 has no ssh key for example.com");
});

it("shows on the card why setting the default branch failed", () => {
  // Given
  const onSetDefaultBranch = cy.stub().rejects(new ConnectError("branch does not exist", Code.NotFound));
  cy.mount(
    aProjectsScreen({
      onSetDefaultBranch,
      loadProjectBranches: () => Promise.resolve({ branches: ["origin/main", "origin/dev"], defaultRemote: "origin" }),
    }),
  );

  // When
  projectsScreenPage.defaultBranchSelect("proj-alpha").select("origin/dev");

  // Then
  projectsScreenPage.cardError("proj-alpha").should("contain.text", "branch does not exist");
});

it("shows on the card why assigning an account failed", () => {
  // Given
  const onSetProjectAccounts = cy.stub().rejects(new ConnectError("vault is locked", Code.FailedPrecondition));
  cy.mount(aProjectsScreen({ onSetProjectAccounts }));

  // When
  projectsScreenPage.accountSelect("proj-alpha", "github").select("acct-1");

  // Then
  projectsScreenPage.cardError("proj-alpha").should("contain.text", "vault is locked");
});

it("shows on the card why its branches could not be loaded", () => {
  // Given
  const loadProjectBranches = () => Promise.reject(new ConnectError("git fetch timed out", Code.DeadlineExceeded));

  // When
  cy.mount(aProjectsScreen({ loadProjectBranches }));

  // Then
  projectsScreenPage.cardError("proj-alpha").should("contain.text", "git fetch timed out");
});

it("shows why the assignable accounts could not be loaded", () => {
  // Given
  const backend = aProjectsBackend((b) =>
    b.failWith(AccountsService.method.listAccounts, Code.Internal, "the vault file is truncated"),
  );

  // When
  mountWithRpc(projectsAppPage(), backend);

  // Then the projects still list; the account pickers say why they are empty
  projectsScreenPage.card("proj-alpha").should("exist");
  projectsScreenPage.accountsLoadError().should("contain.text", "the vault file is truncated");
});
