/**
 * Acceptance tests: soft-deleted projects on the Projects screen (`/projects`). They are left out
 * by default; "Show deleted projects" lists them as deleted cards — who deleted them, when, and that
 * their data is kept — each with **Restore** (`RestoreProject`).
 *
 * Exercised against the `ProjectsContent` container over the in-memory backend, whose
 * `ListProjects` answers the way the daemon does: deleted rows only when asked for.
 *
 * Feature doc: docs/ft/web/projects-screen-multi-host.md § Deleted projects.
 * Changeset: docs/dev/1-WIP/2026-11-15-project-update-and-soft-delete.md
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { aClientsPortDouble, anAuthDouble } from "tddy-client-web/testkit";
import type { DaemonHost } from "tddy-client-web";
import { ProjectService, type ProjectEntry } from "tddy-client-web/gen/project_pb";
import { ProjectsContent } from "../../src/projects/ProjectsContent";
import { projectsScreenPage as page } from "../support/pages/projectsScreenPage";

const HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [{ instanceId: HOST, label: "workstation-1 (this daemon)" }];

const LIVE = "proj-live";
const GONE = "proj-gone";

function aProject(overrides: Partial<ProjectEntry>): ProjectEntry {
  return {
    projectId: LIVE,
    name: "live",
    gitUrl: "https://example.com/live.git",
    mainRepoPath: "/home/dev/repos/live",
    daemonInstanceId: HOST,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    deletedAt: "",
    deletedBy: "",
    restoredAt: "",
    ...overrides,
  } as ProjectEntry;
}

/**
 * A daemon holding one live and one deleted project. `ListProjects` leaves the deleted one out
 * unless `includeDeleted`; `RestoreProject` clears its tombstone, or is refused with `refusal`.
 */
function aDaemon(refusal?: ConnectError): InMemoryRpcBackend {
  const state = [
    aProject({}),
    aProject({
      projectId: GONE,
      name: "gone",
      deletedAt: "2026-11-15T10:00:00.000Z",
      deletedBy: "octocat",
    }),
  ];
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, (req) => ({
      projects: state.filter((p) => req.includeDeleted || p.deletedAt === ""),
    }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(ProjectService.method.restoreProject, (req) => {
      if (refusal) throw refusal;
      const project = state.find((p) => p.projectId === req.projectId)!;
      project.deletedAt = "";
      project.deletedBy = "";
      project.restoredAt = "2026-11-15T11:00:00.000Z";
      return { project, appliedHosts: [HOST] };
    });
}

function mountProjects(backend: InMemoryRpcBackend) {
  return cy.mountWithClients(<ProjectsContent />, backend, {
    clients: aClientsPortDouble(backend, { daemons: DAEMON_HOSTS, selectedInstanceId: HOST }),
    auth: anAuthDouble({ sessionToken: "fake-token" }),
  });
}

const listRequests = (backend: InMemoryRpcBackend) =>
  cy.wrap(backend).then((b) => b.callsTo(ProjectService.method.listProjects));

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("leaves deleted projects out by default", () => {
  // When
  const backend = aDaemon();
  mountProjects(backend);

  // Then
  page.card(LIVE).should("exist");
  page.deletedCard(GONE).should("not.exist");
  page.card(GONE).should("not.exist");
  listRequests(backend).should((requests) => {
    expect(requests.length, "the list was read").to.be.greaterThan(0);
    expect(requests.every((r) => !r.includeDeleted), "deleted projects not asked for").to.equal(true);
  });
});

it("Show deleted projects lists a deleted project as deleted: who, when, and that its data is kept", () => {
  // Given
  mountProjects(aDaemon());
  page.card(LIVE).should("exist");

  // When
  page.setShowDeleted(true);

  // Then the deleted project is a deleted card, not an ordinary one
  page.deletedCard(GONE).should("contain", "gone");
  page.deletedCard(GONE).should("contain", "octocat");
  page.deletedCard(GONE).should("contain", "kept");
  page.card(GONE).should("not.exist");
  // And the live project is still an ordinary card
  page.card(LIVE).should("exist");
});

it("Restore brings a deleted project back as an ordinary card", () => {
  // Given
  const backend = aDaemon();
  mountProjects(backend);
  page.setShowDeleted(true);
  page.deletedCard(GONE).should("exist");

  // When
  page.restore(GONE);

  // Then
  page.card(GONE).should("exist");
  page.deletedCard(GONE).should("not.exist");
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ProjectService.method.restoreProject);
    expect(calls, "one RestoreProject request").to.have.length(1);
    expect(calls[0]?.projectId).to.equal(GONE);
    expect(calls[0]?.localOnly, "fanned out to every host").to.equal(false);
  });
});

it("a refused restore shows the daemon's reason on the deleted card", () => {
  mountProjects(aDaemon(new ConnectError("permission denied", Code.PermissionDenied)));
  page.setShowDeleted(true);

  page.restore(GONE);

  page.restoreError(GONE).should("contain", "permission denied");
  page.deletedCard(GONE).should("exist");
});
