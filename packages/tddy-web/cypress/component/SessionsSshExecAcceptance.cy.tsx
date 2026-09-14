/**
 * Acceptance tests: the new-session form lets an operator run a co-located session's exec catalog
 * on an OpenSSH Host alias of the session host (Host A). Empty is LocalShell. The list is
 * `ListSshConfigHosts` on that host (tests inject a double — this node does not parse config).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-14-ssh-exec.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import {
  HostService,
  ListSshConfigHostsResponseSchema,
  ProbeOutcome,
  SshConfigHostSchema,
} from "../../src/gen/host_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { createSessionPage } from "../support/pages/createSessionPage";
import { mountWithRpc } from "../support/rpc/inMemory";
import { withSelectedDaemonServedBy } from "../support/rpc/withSelectedDaemon";

const SESSION_HOST = "laptop-a";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: SESSION_HOST, label: "laptop-a (this daemon)" },
];

const THIS_HOST = "";

function aListingOf(aliases: string[]) {
  return create(ListSshConfigHostsResponseSchema, {
    outcome: ProbeOutcome.OK,
    hosts: aliases.map((alias) => create(SshConfigHostSchema, { alias })),
  });
}

function aCreateSessionBackend(aliases: string[] = ["buildbox"]): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "ssh-exec-1" }))
    .implement(HostService, {
      listSshConfigHosts: async (req) => {
        expect(req.daemonInstanceId, "n2 lists Host A's aliases").to.equal(SESSION_HOST);
        return aListingOf(aliases);
      },
    });
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(SessionService, backend.transport());
  const projectClient = createClient(ProjectService, backend.transport());
  const catalogClient = createClient(CatalogService, backend.transport());
  const sessionFilesClient = createClient(SessionFilesService, backend.transport());
  const worktreeClient = createClient(WorktreeService, backend.transport());
  mountWithRpc(
    withSelectedDaemonServedBy(
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
      DAEMON_HOSTS,
      SESSION_HOST,
    ),
    backend,
  );
}

function theStartSessionRequest(backend: InMemoryRpcBackend) {
  const calls = backend.callsTo(SessionService.method.startSession);
  expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
  return calls[0];
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

describe("Create session SSH exec", () => {
  it("lists Host A's aliases and an empty local choice", () => {
    // Given
    mountCreatePane(aCreateSessionBackend(["buildbox"]));
    createSessionPage.switchToClaudeCliSession();

    // When
    createSessionPage.enableManagedCodebase();

    // Then
    createSessionPage.sshConfigSelect().should("be.visible");
    createSessionPage.sshConfigOptionValues().should("deep.equal", [THIS_HOST, "buildbox"]);
    createSessionPage.sshConfigOptionLabels().then((labels) => {
      expect(labels[0]).to.equal("This host");
      expect(labels).to.include("buildbox");
    });
  });

  it("sends the chosen alias on StartSession and keeps empty as local", () => {
    // Given
    const backend = aCreateSessionBackend(["buildbox"]);
    mountCreatePane(backend);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject("proj-1");
    createSessionPage.enableManagedCodebase();

    // When
    createSessionPage.selectSshConfigHost("buildbox");
    createSessionPage.submit();

    // Then
    cy.wrap(null).should(() => {
      const request = theStartSessionRequest(backend);
      expect(request.sessionType).to.equal("claude-cli");
      expect(request.managedCodebase).to.equal(true);
      expect(request.sshConfigHost).to.equal("buildbox");
    });
  });

  it("does not offer SSH execution on a cursor-cli session", () => {
    // Given
    mountCreatePane(aCreateSessionBackend());
    createSessionPage.switchToCursorCliSession();

    // When
    createSessionPage.enableManagedCodebase();

    // Then — cursor-agent cannot drop native FS tools
    createSessionPage.expectNoSshConfigSelector();
  });
});
