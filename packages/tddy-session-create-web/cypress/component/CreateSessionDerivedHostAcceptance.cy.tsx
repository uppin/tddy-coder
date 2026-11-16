/**
 * Acceptance tests: the new-session form (CreateSessionPane) offers the hosts a daemon reaches
 * through a remote shell — its SSH config aliases — in the Host field, under that daemon, and
 * starts a session on one.
 *
 * A derived host has no daemon. Choosing it starts the session on its **parent** — the daemon
 * that reaches it — with the derived host as the session's `target`, so `StartSession` carries the
 * parent as `daemonInstanceId`, never the derived host's own id. Its agent runs on a machine with
 * no tddy software, so permission prompts are answered in the terminal, and the form says so.
 *
 * The derived host arrives the way every host does — in the directory's `daemons`, here through
 * the clients port double — carrying `parent` and `target`. The RPCs the form issues go to the
 * in-memory ConnectRPC backend, so the tests assert on the typed request the component sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-derived-hosts.md.
 */

import { aClientsPortDouble } from "tddy-client-web/testkit";
import React from "react";
import { create } from "@bufbuild/protobuf";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/CreateSessionPane";
import { SessionService, SessionTargetSchema } from "tddy-client-web/gen/session_pb";
import { ProjectService } from "tddy-client-web/gen/project_pb";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { DaemonHost } from "tddy-client-web";
import { createSessionPage } from "../support/pages/createSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The daemon that reaches the derived host — its parent. */
const PARENT_DAEMON = "workstation-1";
/** The SSH config alias the derived host is known by on its parent. */
const BUILDBOX_ALIAS = "buildbox";
/** The derived host's id: its parent, its kind, its alias. */
const BUILDBOX = `${PARENT_DAEMON}/ssh/${BUILDBOX_ALIAS}`;

/** One daemon, and the SSH host it reaches, as the host directory lists them. */
const HOSTS: DaemonHost[] = [
  { instanceId: PARENT_DAEMON, label: "workstation-1 (this daemon)" },
  {
    instanceId: BUILDBOX,
    label: BUILDBOX_ALIAS,
    parent: PARENT_DAEMON,
    target: create(SessionTargetSchema, {
      kind: { case: "ssh", value: { configHost: BUILDBOX_ALIAS } },
    }),
  },
];

/** The Agent select's option value: the agent, on the daemon that offers it. */
const CLAUDE_OPTION = `claude@${PARENT_DAEMON}`;

/** A backend seeded with every RPC CreateSessionPane issues, plus StartSession. */
function aCreateSessionBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({
        agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude") }],
      }),
    )
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "derived-host-1" }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(SessionService, backend.transport());
  const projectClient = createClient(ProjectService, backend.transport());
  const catalogClient = createClient(CatalogService, backend.transport());
  const sessionFilesClient = createClient(SessionFilesService, backend.transport());
  const worktreeClient = createClient(WorktreeService, backend.transport());
  cy.mountWithClients(
    <CreateSessionPane
      client={client}
      projectClient={projectClient}
      catalogClient={catalogClient}
      sessionFilesClient={sessionFilesClient}
      worktreeClient={worktreeClient}
      sessionToken="fake-token"
      onCancel={cy.stub()}
      onCreated={cy.stub()}
    />,
    backend,
    { clients: aClientsPortDouble(backend, { daemons: HOSTS }) },
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

it("offers a derived host under the daemon that reaches it", () => {
  // Given a new-session form over one daemon and the SSH host it reaches
  mountCreatePane(aCreateSessionBackend());

  // Then the daemon is offered as a host of its own, and the SSH host in its group, by its alias
  createSessionPage.daemonHostOptionValues().should("deep.equal", [PARENT_DAEMON]);
  createSessionPage
    .derivedHostOptions(PARENT_DAEMON)
    .should("deep.equal", [[BUILDBOX, BUILDBOX_ALIAS]]);
});

it("starts on the parent daemon with the derived host as the target", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);

  // When — fill the required fields, pick the derived host, and create
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent(CLAUDE_OPTION);
  createSessionPage.selectHost(BUILDBOX);
  createSessionPage.submit();

  // Then — StartSession goes to the daemon that reaches the host, naming the host as its target
  cy.wrap(null).should(() => {
    const starts = backend.callsTo(SessionService.method.startSession).map((start) => ({
      daemonInstanceId: start.daemonInstanceId,
      targetKind: start.target?.kind.case,
      sshConfigHost: start.target?.kind.value?.configHost,
    }));
    expect(starts).to.deep.equal([
      { daemonInstanceId: PARENT_DAEMON, targetKind: "ssh", sshConfigHost: BUILDBOX_ALIAS },
    ]);
  });
});

it("says that prompts are answered in the terminal when a derived host is chosen", () => {
  // Given
  mountCreatePane(aCreateSessionBackend());

  // When
  createSessionPage.selectHost(BUILDBOX);

  // Then
  createSessionPage
    .derivedHostNote()
    .should("be.visible")
    .and("contain.text", "answered in the terminal");
});
