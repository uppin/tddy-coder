/**
 * Acceptance tests: the new-session form's "Project" dropdown offers one entry per *logical project*.
 *
 * Aggregated `ListProjects` returns one row per (project_id, hosting daemon) — see
 * `packages/tddy-daemon/tests/list_projects_multi_daemon_aggregation.rs`, where that is the pinned
 * contract. The form's Project selector submits only a `project_id` (the host is chosen by its own
 * "Host" selector), so a project carried by two daemons must appear once, not once per host.
 *
 * Two genuinely different projects that share a name are the opposite case: both stay, and each label
 * carries its project id in parentheses so the operator can tell which one they are picking.
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const REMOTE_HOST = "server-2";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
  { instanceId: REMOTE_HOST, label: "server-2" },
];

/** One aggregated `ListProjects` row: a project as one host's registry carries it. */
interface ProjectRowFixture {
  projectId: string;
  name: string;
  daemonInstanceId: string;
}

/** A backend seeded with every RPC CreateSessionPane issues, listing `projects` as aggregated rows. */
function aCreateSessionBackend(projects: ProjectRowFixture[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: projects.map((p) => ({
        projectId: p.projectId,
        name: p.name,
        mainRepoPath: `/repo/${p.projectId}`,
        daemonInstanceId: p.daemonInstanceId,
      })),
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: ["origin/master"],
      defaultRemote: "origin",
    }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />
    </SelectedDaemonProvider>,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("offers one plainly named option per project when a project is registered on two hosts", () => {
  // Given — tddy-coder is carried by both daemons, tddy-web only by the local one
  mountCreatePane(
    aCreateSessionBackend([
      { projectId: "proj-tddy-coder", name: "tddy-coder", daemonInstanceId: LOCAL_HOST },
      { projectId: "proj-tddy-coder", name: "tddy-coder", daemonInstanceId: REMOTE_HOST },
      { projectId: "proj-tddy-web", name: "tddy-web", daemonInstanceId: LOCAL_HOST },
    ]),
  );
  createSessionPage.awaitProjectOption("proj-tddy-web");

  // When / Then — the two host rows of tddy-coder read as the one project they are, and neither
  // name is ambiguous, so no id is appended to either caption
  createSessionPage
    .projectOptionValues()
    .should("deep.equal", ["proj-tddy-coder", "proj-tddy-web"]);
  createSessionPage.projectOptionLabels().should("deep.equal", ["tddy-coder", "tddy-web"]);
});

it("shows each project's id in parentheses when two projects share a name", () => {
  // Given — two unrelated checkouts of a same-named repo, on the same host
  mountCreatePane(
    aCreateSessionBackend([
      { projectId: "proj-tddy-coder-oss", name: "tddy-coder", daemonInstanceId: LOCAL_HOST },
      { projectId: "proj-tddy-coder-fork", name: "tddy-coder", daemonInstanceId: LOCAL_HOST },
    ]),
  );
  createSessionPage.awaitProjectOption("proj-tddy-coder-fork");

  // When / Then — the shared name alone is ambiguous, so each label carries its id
  createSessionPage
    .projectOptionLabels()
    .should("deep.equal", [
      "tddy-coder (proj-tddy-coder-oss)",
      "tddy-coder (proj-tddy-coder-fork)",
    ]);
});

it("pre-selects the project when its rows from two hosts are the only choice", () => {
  // Given — one logical project, carried by both daemons
  mountCreatePane(
    aCreateSessionBackend([
      { projectId: "proj-tddy-coder", name: "tddy-coder", daemonInstanceId: LOCAL_HOST },
      { projectId: "proj-tddy-coder", name: "tddy-coder", daemonInstanceId: REMOTE_HOST },
    ]),
  );

  // When / Then — there is no decision to make, so the form makes it (as it does for one row)
  createSessionPage.projectSelect().should("have.value", "proj-tddy-coder");
});
