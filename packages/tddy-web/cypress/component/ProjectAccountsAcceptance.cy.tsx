/**
 * Acceptance tests: the Projects screen assigns provider accounts to a project.
 *
 * A project card carries one assignment row per provider the caller's vault holds accounts at.
 * Each row's `<select>` offers that provider's accounts plus an empty-valued "no account assigned"
 * option — which is a **real choice**, not a placeholder: a provider with no assignment resolves to
 * nothing, and the daemon has no fallback to the caller's own login or to a sole vault account.
 *
 * Assignment is set-valued, not field-valued: `SetProjectAccounts` replaces a project's whole set,
 * so changing one provider's row sends every other provider's assignment along unchanged.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-09-19-keyring-assignments.md § Proposed Changes.
 */

import React from "react";
import { Room } from "livekit-client";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import {
  ProjectsScreen,
  type AssignableAccount,
} from "../../src/components/projects/ProjectsScreen";
import { type ProjectEntry } from "../../src/gen/project_pb";
import { ProjectService } from "../../src/gen/project_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const PROJECT_ID = "proj-alpha";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
];

function anAccount(provider: string, accountId: string, label: string): AssignableAccount {
  return { provider, accountId, label };
}

function anAssignment(provider: string, accountId: string) {
  return { provider, accountId };
}

function aProject(overrides: Partial<ProjectEntry>): ProjectEntry {
  return {
    projectId: PROJECT_ID,
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
  Promise.resolve<{ branches: string[]; defaultRemote: string }>({
    branches: [],
    defaultRemote: "",
  });

/** The presentational screen, mounted with an explicit vault and an explicit assignment set. */
function aProjectsScreen(options: {
  project: ProjectEntry;
  accounts: AssignableAccount[];
  onSetProjectAccounts?: (input: {
    projectId: string;
    accounts: { provider: string; accountId: string }[];
    daemonInstanceId: string;
  }) => void;
}) {
  return (
    <ProjectsScreen
      projects={[options.project]}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub()}
      onAddProjectToHost={cy.stub()}
      onSetDefaultBranch={cy.stub()}
      loadProjectBranches={noBranches}
      accounts={options.accounts}
      onSetProjectAccounts={options.onSetProjectAccounts ?? cy.stub()}
    />
  );
}

function aProjectsBackend(projects: ProjectEntry[]): InMemoryRpcBackend {
  const state = [...projects];
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects: state }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: [],
      defaultRemote: "origin",
    }))
    .onUnary(ProjectService.method.setProjectAccounts, (req) => {
      for (const p of state) {
        if (p.projectId === req.projectId) p.accounts = req.accounts;
      }
      return { project: state.find((p) => p.projectId === req.projectId)! };
    });
}

function mountProjectsAppPage() {
  return (
    <AuthProvider>
      <SelectedDaemonProvider
        room={new Room()}
        daemons={DAEMON_HOSTS}
        servingInstanceId={LOCAL_HOST}
      >
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// What a project card offers
// ---------------------------------------------------------------------------

it("offers one assignment row per provider the vault holds an account at", () => {
  // Given a vault holding two github accounts and one cloudflare account
  cy.mount(
    aProjectsScreen({
      project: aProject({}),
      accounts: [
        anAccount("github", "acct-octocat", "Octocat"),
        anAccount("github", "acct-hubot", "Hubot"),
        anAccount("cloudflare", "acct-zone", "Zone admin"),
      ],
    }),
  );

  // Then — one row per provider, not one per account
  projectsScreenPage
    .accountRowProviders(PROJECT_ID)
    .should("deep.equal", ["github", "cloudflare"]);
});

it("offers a provider only its own accounts, alongside the unassigned option", () => {
  // Given
  cy.mount(
    aProjectsScreen({
      project: aProject({}),
      accounts: [
        anAccount("github", "acct-octocat", "Octocat"),
        anAccount("cloudflare", "acct-zone", "Zone admin"),
      ],
    }),
  );

  // Then — the empty-valued unassigned option leads, then this provider's accounts
  projectsScreenPage
    .accountOptionValues(PROJECT_ID, "github")
    .should("deep.equal", ["", "acct-octocat"]);
  projectsScreenPage
    .accountOptionLabels(PROJECT_ID, "github")
    .should("deep.equal", ["No account assigned", "Octocat"]);
});

it("holds no assignment row for a provider the vault has no account at", () => {
  // Given a vault holding github accounts only
  cy.mount(
    aProjectsScreen({
      project: aProject({}),
      accounts: [anAccount("github", "acct-octocat", "Octocat")],
    }),
  );

  // Then
  projectsScreenPage.accountRow(PROJECT_ID, "cloudflare").should("not.exist");
});

// ---------------------------------------------------------------------------
// What a project card shows as assigned
// ---------------------------------------------------------------------------

it("shows a project with no assignment at a provider as unassigned", () => {
  // Given
  cy.mount(
    aProjectsScreen({
      project: aProject({ accounts: [] }),
      accounts: [anAccount("github", "acct-octocat", "Octocat")],
    }),
  );

  // Then
  projectsScreenPage.assignedAccountId(PROJECT_ID, "github").should("equal", "");
});

it("shows the assigned account selected at its own provider", () => {
  // Given a project assigned a github account and nothing at cloudflare
  cy.mount(
    aProjectsScreen({
      project: aProject({ accounts: [anAssignment("github", "acct-hubot")] } as Partial<ProjectEntry>),
      accounts: [
        anAccount("github", "acct-octocat", "Octocat"),
        anAccount("github", "acct-hubot", "Hubot"),
        anAccount("cloudflare", "acct-zone", "Zone admin"),
      ],
    }),
  );

  // Then
  projectsScreenPage.assignedAccountId(PROJECT_ID, "github").should("equal", "acct-hubot");
  projectsScreenPage.assignedAccountId(PROJECT_ID, "cloudflare").should("equal", "");
});

// ---------------------------------------------------------------------------
// Changing an assignment
// ---------------------------------------------------------------------------

it("sends every provider's assignment when one provider's account is chosen", () => {
  // Given a project already assigned a cloudflare account
  const onSetProjectAccounts = cy.stub().as("setProjectAccounts");
  cy.mount(
    aProjectsScreen({
      project: aProject({
        accounts: [anAssignment("cloudflare", "acct-zone")],
      } as Partial<ProjectEntry>),
      accounts: [
        anAccount("github", "acct-octocat", "Octocat"),
        anAccount("cloudflare", "acct-zone", "Zone admin"),
      ],
      onSetProjectAccounts,
    }),
  );

  // When a github account is assigned
  projectsScreenPage.assignAccount(PROJECT_ID, "github", "acct-octocat");

  // Then — the whole set travels, because the daemon replaces rather than merges
  cy.get("@setProjectAccounts").should("have.been.calledWithMatch", {
    projectId: PROJECT_ID,
    accounts: [anAssignment("cloudflare", "acct-zone"), anAssignment("github", "acct-octocat")],
    daemonInstanceId: LOCAL_HOST,
  });
});

it("sends the remaining assignments when a provider is returned to unassigned", () => {
  // Given a project assigned an account at each of two providers
  const onSetProjectAccounts = cy.stub().as("setProjectAccounts");
  cy.mount(
    aProjectsScreen({
      project: aProject({
        accounts: [anAssignment("github", "acct-octocat"), anAssignment("cloudflare", "acct-zone")],
      } as Partial<ProjectEntry>),
      accounts: [
        anAccount("github", "acct-octocat", "Octocat"),
        anAccount("cloudflare", "acct-zone", "Zone admin"),
      ],
      onSetProjectAccounts,
    }),
  );

  // When github is returned to unassigned
  projectsScreenPage.assignAccount(PROJECT_ID, "github", "");

  // Then — github is absent from the set rather than carried as an empty account id
  cy.get("@setProjectAccounts").should("have.been.calledWithMatch", {
    projectId: PROJECT_ID,
    accounts: [anAssignment("cloudflare", "acct-zone")],
  });
});

// ---------------------------------------------------------------------------
// Container behavior (RPC wiring)
// ---------------------------------------------------------------------------

it("assigns an account over the daemon and shows it after the list refreshes", () => {
  // Given a daemon serving one unassigned project and a vault holding one github account
  const backend = aProjectsBackend([aProject({})]).onUnary(
    ProjectService.method.listProjectBranches,
    () => ({ branches: [], defaultRemote: "origin" }),
  );

  // When
  mountWithRpc(mountProjectsAppPage(), backend);
  projectsScreenPage.assignAccount(PROJECT_ID, "github", "acct-octocat");

  // Then
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ProjectService.method.setProjectAccounts)).to.have.length(1);
  });
  projectsScreenPage.assignedAccountId(PROJECT_ID, "github").should("equal", "acct-octocat");
});
