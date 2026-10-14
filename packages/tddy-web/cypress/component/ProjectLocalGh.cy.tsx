/**
 * The Projects screen sets a project to **Local gh**: its sessions take their GitHub identity from
 * each host's own `gh` login instead of a vault account.
 *
 * The setting travels with the project's assignments in one `SetProjectAccounts` write, and a
 * project has one GitHub source: turning Local gh on drops the GitHub assignment, keeps every other
 * provider's, and hides the GitHub account picker. Each host of a Local gh project shows whether its
 * `gh` is logged in for the project's GitHub host — the host named by the project's git URL.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-14-local-gh-github-source.md
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import {
  ProjectsScreen,
  type AssignableAccount,
} from "../../src/components/projects/ProjectsScreen";
import { type ProjectEntry, ProjectService } from "../../src/gen/project_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import {
  HostGithubCliSchema,
  HostService,
  ProbeOutcome,
  type HostGithubCli,
} from "../../src/gen/host_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSTATION = "workstation-1";
const BUILD_BOX = "build-box-2";
const PROJECT_ID = "proj-alpha";
const ENTERPRISE_GIT_URL = "git@github.example.com:acme/alpha.git";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: WORKSTATION, label: "workstation-1 (this daemon)" },
  { instanceId: BUILD_BOX, label: "build-box-2" },
];

const VAULT: AssignableAccount[] = [
  { provider: "github", accountId: "acct-octocat", label: "Octocat" },
  { provider: "cloudflare", accountId: "acct-edge", label: "Edge zone" },
];

function anAssignment(provider: string, accountId: string) {
  return { provider, accountId };
}

/** One host's row of a project, as `ListProjects` returns it. */
function aProjectRow(overrides: Partial<ProjectEntry>): ProjectEntry {
  return {
    projectId: PROJECT_ID,
    name: "alpha",
    gitUrl: "https://github.com/acme/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: WORKSTATION,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    localGh: false,
    ...overrides,
  } as ProjectEntry;
}

/** A `gh` logged in as `login` for `hostname` only; any other host reads as logged out. */
function aGhLoggedInFor(hostname: string, login: string) {
  return (input: { githubHostname: string }): Promise<HostGithubCli> =>
    Promise.resolve(
      create(HostGithubCliSchema, {
        outcome: ProbeOutcome.OK,
        installed: true,
        authenticated: input.githubHostname === hostname,
        login: input.githubHostname === hostname ? login : "",
      }),
    );
}

const aLoggedOutGh = (): Promise<HostGithubCli> =>
  Promise.resolve(
    create(HostGithubCliSchema, { outcome: ProbeOutcome.OK, installed: true, authenticated: false }),
  );

const noBranches = () =>
  Promise.resolve<{ branches: string[]; defaultRemote: string }>({ branches: [], defaultRemote: "" });

function aProjectsScreen(options: {
  rows: ProjectEntry[];
  onSetProjectAccounts?: ReturnType<typeof cy.stub>;
  loadHostGithubCli?: (input: {
    daemonInstanceId: string;
    githubHostname: string;
  }) => Promise<HostGithubCli>;
}) {
  return (
    <ProjectsScreen
      projects={options.rows}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub()}
      onAddProjectToHost={cy.stub()}
      onSetDefaultBranch={cy.stub()}
      loadProjectBranches={noBranches}
      accounts={VAULT}
      onSetProjectAccounts={options.onSetProjectAccounts ?? cy.stub()}
      loadHostGithubCli={options.loadHostGithubCli ?? aLoggedOutGh}
    />
  );
}

/** A daemon serving `rows`, whose `SetProjectAccounts` stores what it is sent. */
function aProjectsBackend(rows: ProjectEntry[]): InMemoryRpcBackend {
  const state = [...rows];
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects: state }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: [],
      defaultRemote: "origin",
    }))
    .onUnary(ProjectService.method.setProjectAccounts, (req) => {
      for (const p of state) {
        if (p.projectId === req.projectId) {
          p.accounts = req.accounts;
          p.localGh = req.localGh;
        }
      }
      return { project: state.find((p) => p.projectId === req.projectId)! };
    })
    .onUnary(AccountsService.method.listAccounts, () => ({
      providers: [
        {
          provider: "github",
          accounts: [{ provider: "github", accountId: "acct-octocat", label: "Octocat" }],
        },
      ],
      vaultLocked: false,
      vaultUninitialized: false,
    }))
    .onUnary(HostService.method.getHostTooling, (req) => ({
      daemonInstanceId: req.daemonInstanceId,
      githubCli: create(HostGithubCliSchema, {
        outcome: ProbeOutcome.OK,
        installed: true,
        authenticated: true,
        login: "ada",
      }),
    }));
}

function mountProjectsAppPage() {
  return (
    <AuthProvider>
      <SelectedDaemonProvider
        room={new Room()}
        daemons={DAEMON_HOSTS.slice(0, 1)}
        servingInstanceId={WORKSTATION}
      >
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Presentational behavior
// ---------------------------------------------------------------------------

it("turns Local gh on and sends the remaining assignments without GitHub", () => {
  // Given a project assigned a GitHub and a Cloudflare account
  const onSetProjectAccounts = cy.stub().as("setProjectAccounts");
  cy.mount(
    aProjectsScreen({
      rows: [
        aProjectRow({
          accounts: [anAssignment("github", "acct-octocat"), anAssignment("cloudflare", "acct-edge")],
        }),
      ],
      onSetProjectAccounts,
    }),
  );

  // When Local gh is turned on
  projectsScreenPage.turnLocalGhOn(PROJECT_ID);

  // Then one write carries Local gh and every assignment but GitHub's
  cy.get("@setProjectAccounts").should("have.been.calledOnceWith", {
    projectId: PROJECT_ID,
    accounts: [anAssignment("cloudflare", "acct-edge")],
    localGh: true,
    daemonInstanceId: WORKSTATION,
  });
});

it("hides the GitHub account picker while Local gh is on", () => {
  // Given a Local gh project, and a vault holding GitHub and Cloudflare accounts
  cy.mount(aProjectsScreen({ rows: [aProjectRow({ localGh: true })] }));

  // When the card is shown
  projectsScreenPage.card(PROJECT_ID).should("be.visible");

  // Then only Cloudflare can be assigned
  projectsScreenPage.accountRowProviders(PROJECT_ID).should("deep.equal", ["cloudflare"]);
});

it("shows the gh login each host has for the project's GitHub host", () => {
  // Given a Local gh project on an Enterprise host, and a host whose gh is logged in there as grace
  cy.mount(
    aProjectsScreen({
      rows: [aProjectRow({ gitUrl: ENTERPRISE_GIT_URL, localGh: true })],
      loadHostGithubCli: aGhLoggedInFor("github.example.com", "grace"),
    }),
  );

  // When the card is shown
  projectsScreenPage.card(PROJECT_ID).should("be.visible");

  // Then the host row says who gh is logged in as
  projectsScreenPage.hostGhState(PROJECT_ID, WORKSTATION).should("contain", "grace");
});

it("shows a host whose gh is not logged in for the project's GitHub host", () => {
  // Given a Local gh project on an Enterprise host, on a host whose gh is logged in to github.com only
  cy.mount(
    aProjectsScreen({
      rows: [aProjectRow({ gitUrl: ENTERPRISE_GIT_URL, localGh: true })],
      loadHostGithubCli: aGhLoggedInFor("github.com", "ada"),
    }),
  );

  // When the card is shown
  projectsScreenPage.card(PROJECT_ID).should("be.visible");

  // Then the host row says gh is not logged in there, and names the host
  projectsScreenPage
    .hostGhState(PROJECT_ID, WORKSTATION)
    .should("contain", "not logged in")
    .and("contain", "github.example.com");
});

it("asks every host of a Local gh project about its own gh", () => {
  // Given a Local gh project on two hosts
  const loadHostGithubCli = cy.stub().as("loadHostGithubCli").callsFake(aLoggedOutGh);
  cy.mount(
    aProjectsScreen({
      rows: [
        aProjectRow({ gitUrl: ENTERPRISE_GIT_URL, localGh: true }),
        aProjectRow({ gitUrl: ENTERPRISE_GIT_URL, localGh: true, daemonInstanceId: BUILD_BOX }),
      ],
      loadHostGithubCli,
    }),
  );

  // When the card is shown
  projectsScreenPage.card(PROJECT_ID).should("be.visible");

  // Then each host is asked about the project's GitHub host
  cy.get("@loadHostGithubCli").should("have.been.calledWith", {
    daemonInstanceId: WORKSTATION,
    githubHostname: "github.example.com",
  });
  cy.get("@loadHostGithubCli").should("have.been.calledWith", {
    daemonInstanceId: BUILD_BOX,
    githubHostname: "github.example.com",
  });
});

// ---------------------------------------------------------------------------
// Container behavior (RPC wiring)
// ---------------------------------------------------------------------------

it("turns Local gh on over the daemon and shows it after the list refreshes", () => {
  // Given a daemon serving one project that uses the vault
  const backend = aProjectsBackend([aProjectRow({})]);

  // When Local gh is turned on
  mountWithRpc(mountProjectsAppPage(), backend);
  projectsScreenPage.turnLocalGhOn(PROJECT_ID);

  // Then the daemon was told, and the refreshed card shows this host's gh login
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ProjectService.method.setProjectAccounts)).to.have.length(1);
  });
  projectsScreenPage.localGhToggle(PROJECT_ID).should("be.checked");
  projectsScreenPage.hostGhState(PROJECT_ID, WORKSTATION).should("contain", "ada");
});
