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
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { POLL_INTERVAL_MS } from "../../src/components/projects/ProjectsAppPage";
import { ProjectsScreen } from "../../src/components/projects/ProjectsScreen";
import { ProjectService } from "../../src/gen/project_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aProject,
  aProjectsBackend,
  PROJECTS_DAEMON_HOSTS as DAEMON_HOSTS,
  PROJECTS_REMOTE_HOST as REMOTE_HOST,
  projectsAppPage,
} from "../support/rpc/projectsAppPageFixtures";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

const noBranches = () =>
  Promise.resolve<{ branches: string[]; defaultRemote: string }>({ branches: [], defaultRemote: "" });

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

// A wire over a room that is not connected — the LiveKit-server-down case. Each surface says the
// transport's own reason, which names the daemon participant the request was addressed to.
const ROOM_NOT_CONNECTED_REASON = "could not send request to daemon-workstation-1: PC manager is closed";

it("shows the transport's reason on the list when the common room never connected", () => {
  // Given / When
  cy.mount(projectsAppPage(new Room()));

  // Then
  projectsScreenPage.loadError().should("have.text", ROOM_NOT_CONNECTED_REASON);
});

it("shows the transport's reason on the create form when the common room never connected", () => {
  // Given
  cy.mount(projectsAppPage(new Room()));

  // When
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({ name: "beta", gitUrl: "https://example.com/beta.git" });

  // Then
  projectsScreenPage.createProjectError().should("have.text", ROOM_NOT_CONNECTED_REASON);
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
  cy.tick(POLL_INTERVAL_MS);

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

it("says accounts are not enabled when the host's daemon serves no accounts service", () => {
  // Given a daemon with no `auth_storage`, which never registers AccountsService
  const backend = aProjectsBackend((b) =>
    b.failWith(
      AccountsService.method.listAccounts,
      Code.NotFound,
      "Unknown service: accounts.AccountsService",
    ),
  );

  // When
  mountWithRpc(projectsAppPage(), backend);

  // Then the reason names the config that turns accounts on, not the router's words
  projectsScreenPage.card("proj-alpha").should("exist");
  projectsScreenPage
    .accountsLoadError()
    .should("contain.text", "Accounts are not enabled on this host")
    .and("contain.text", "auth_storage")
    .and("not.contain.text", "Unknown service");
});
