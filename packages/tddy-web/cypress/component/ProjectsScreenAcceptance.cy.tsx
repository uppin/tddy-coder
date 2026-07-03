/**
 * Acceptance tests: the dedicated Projects screen (`/projects`) lists projects
 * grouped by host and lets an operator add an existing project to another host
 * (reusing its project id) — hosts sourced from connected daemon participants.
 *
 * PRD: docs/ft/web/projects-screen-multi-host.md. Changeset:
 * docs/dev/1-WIP/multi-host-projects.md.
 */

import React from "react";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import {
  ConnectionService,
  type ProjectEntry,
  type EligibleDaemonEntry,
} from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const REMOTE_HOST = "server-2";

const DAEMONS: Partial<EligibleDaemonEntry>[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)", isLocal: true },
  { instanceId: REMOTE_HOST, label: "server-2", isLocal: false },
];

function aProject(overrides: Partial<ProjectEntry>): Partial<ProjectEntry> {
  return {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: LOCAL_HOST,
    ...overrides,
  };
}

/**
 * In-memory backend pre-seeded with the RPCs `ProjectsAppPage` calls on startup.
 * `projects` is mutated by the create/add handlers so the screen's poll reflects
 * writes (Cypress retries assertions until the next poll picks them up).
 */
function aProjectsBackend(projects: Partial<ProjectEntry>[]): InMemoryRpcBackend {
  const state = [...projects];
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: state }))
    .onUnary(ConnectionService.method.listEligibleDaemons, () => ({ daemons: DAEMONS }))
    .onUnary(ConnectionService.method.createProject, (req) => {
      const project = aProject({
        projectId: "proj-new",
        name: req.name,
        gitUrl: req.gitUrl,
        daemonInstanceId: LOCAL_HOST,
      });
      state.push(project);
      return { project };
    })
    .onUnary(ConnectionService.method.addProjectToHost, (req) => {
      const project = aProject({
        projectId: req.projectId,
        name: req.name,
        gitUrl: req.gitUrl,
        daemonInstanceId: req.daemonInstanceId,
      });
      state.push(project);
      return { project };
    });
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
// Tests
// ---------------------------------------------------------------------------

it("renders a project present on two hosts as one card with a row per host", () => {
  // Given
  const backend = aProjectsBackend([
    aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST, mainRepoPath: "/home/dev/alpha" }),
    aProject({ projectId: "proj-alpha", daemonInstanceId: REMOTE_HOST, mainRepoPath: "/srv/alpha" }),
  ]);

  // When
  mountWithRpc(<ProjectsAppPage onNavigate={cy.stub()} />, backend);

  // Then
  projectsScreenPage.card("proj-alpha").should("exist");
  projectsScreenPage
    .hostRowDaemonIds("proj-alpha")
    .should("deep.equal", [LOCAL_HOST, REMOTE_HOST]);
});

it("adds a project to the selected host reusing the project's existing id", () => {
  // Given
  const backend = aProjectsBackend([
    aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST }),
  ]);

  // When
  mountWithRpc(<ProjectsAppPage onNavigate={cy.stub()} />, backend);
  projectsScreenPage.openAddToHost("proj-alpha");
  projectsScreenPage.addProjectToHost("proj-alpha", REMOTE_HOST);

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.addProjectToHost);
    expect(calls).to.have.length(1);
    expect(calls[0].projectId).to.equal("proj-alpha");
    expect(calls[0].daemonInstanceId).to.equal(REMOTE_HOST);
  });
});

it("offers only hosts that do not already host the project as add-to-host targets", () => {
  // Given — the project already lives on the local host
  const backend = aProjectsBackend([
    aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST }),
  ]);

  // When
  mountWithRpc(<ProjectsAppPage onNavigate={cy.stub()} />, backend);
  projectsScreenPage.openAddToHost("proj-alpha");

  // Then — only the remote host (not the already-hosting local host) is selectable
  projectsScreenPage.addToHostOptionValues("proj-alpha").should("deep.equal", [REMOTE_HOST]);
});

it("creates a project from the screen and shows it after the list refreshes", () => {
  // Given
  const backend = aProjectsBackend([]);

  // When
  mountWithRpc(<ProjectsAppPage onNavigate={cy.stub()} />, backend);
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({
    name: "beta",
    gitUrl: "https://example.com/beta.git",
  });

  // Then
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.createProject)).to.have.length(1);
  });
  projectsScreenPage.card("proj-new").should("exist");
});
