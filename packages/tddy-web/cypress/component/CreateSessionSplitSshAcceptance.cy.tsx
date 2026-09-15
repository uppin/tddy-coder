/**
 * Acceptance tests: a split session's SSH dropdown lists Host aliases from the *codebase* host
 * (B), not the agent host (A). Co-located listing stays A's. The chosen alias rides on
 * StartSession.sshConfigHost and is forwarded on the workspace half — A does not open ssh(1).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-09-14-split-ssh.md
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
import { AuthProvider } from "../../src/hooks/authProvider";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aJoinedCommonRoom } from "../support/rpc/withSelectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

const AGENT_HOST = "laptop-a";
const CODEBASE_HOST = "workstation-b";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: AGENT_HOST, label: "laptop-a (this daemon)" },
  { instanceId: CODEBASE_HOST, label: "workstation-b" },
];

const THIS_HOST = "";

function aListingOf(aliases: string[]) {
  return create(ListSshConfigHostsResponseSchema, {
    outcome: ProbeOutcome.OK,
    hosts: aliases.map((alias) => create(SshConfigHostSchema, { alias })),
  });
}

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
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "split-ssh-1" }))
    .implement(HostService, {
      listSshConfigHosts: async (req) => {
        if (req.daemonInstanceId === CODEBASE_HOST) {
          return aListingOf(["jumpbox"]);
        }
        return aListingOf(["buildbox"]);
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
    <AuthProvider>
      <SelectedDaemonProvider
        room={aJoinedCommonRoom([
          daemonRpcIdentity(AGENT_HOST),
          daemonRpcIdentity(CODEBASE_HOST),
        ])}
        daemons={DAEMON_HOSTS}
        servingInstanceId={AGENT_HOST}
      >
        <CreateSessionPane
          client={client}
          projectClient={projectClient}
          catalogClient={catalogClient}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          sessionToken="fake-token"
          onCancel={cy.stub()}
          onCreated={cy.stub()}
        />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

describe("Create session split SSH", () => {
  let rpcBackend: InMemoryRpcBackend;

  it("lists the codebase host's aliases when the worktree is placed on another daemon", () => {
    // Given A names buildbox and B names jumpbox
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.enableManagedCodebase();

    // When the operator places the codebase on workstation-b
    createSessionPage.selectCodebaseHost(CODEBASE_HOST);
    createSessionPage.codebaseHostSelect().should("have.value", CODEBASE_HOST);

    // Then the SSH dropdown is B's list — A must not be the OpenSSH client
    createSessionPage.sshConfigSelect().should("be.visible");
    cy.wrap(rpcBackend).should(() => {
      const calls = rpcBackend.callsTo(HostService.method.listSshConfigHosts);
      expect(calls.some((call) => call.daemonInstanceId === CODEBASE_HOST)).to.equal(true);
    });
    createSessionPage.sshConfigOptionValues().should("deep.equal", [THIS_HOST, "jumpbox"]);
    createSessionPage.sshConfigOptionLabels().then((labels) => {
      expect(labels[0]).to.equal("This host");
      expect(labels).to.include("jumpbox");
      expect(labels).to.not.include("buildbox");
    });
  });

  it("lists the session host's aliases when the session is co-located", () => {
    // Given A names buildbox and B names jumpbox
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend);
    createSessionPage.switchToClaudeCliSession();

    // When the operator keeps the checkout on this host
    createSessionPage.enableManagedCodebase();

    // Then the SSH dropdown is A's list
    createSessionPage.sshConfigSelect().should("be.visible");
    createSessionPage.sshConfigOptionValues().should("deep.equal", [THIS_HOST, "buildbox"]);
    createSessionPage.sshConfigOptionLabels().then((labels) => {
      expect(labels[0]).to.equal("This host");
      expect(labels).to.include("buildbox");
      expect(labels).to.not.include("jumpbox");
    });
  });
});
