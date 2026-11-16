/**
 * Acceptance tests: the new-session form (CreateSessionPane) offers the workspaces of a person's
 * Coder accounts in the Host field, under the daemon that reaches them, starts a session on a
 * running one, and does not let one that cannot be used be chosen.
 *
 * A workspace has no daemon. Choosing it starts the session on its **parent** with the workspace
 * as the session's `target` — the account, the owner and the workspace — so `StartSession` carries
 * the parent as `daemonInstanceId`. A stopped workspace stays in the list, disabled, with the
 * reason its daemon gave: hiding it would read as an account that has no workspaces.
 *
 * The workspaces arrive the way every host does — in the directory's `daemons`, here through the
 * clients port double — carrying `parent`, `target`, `usable` and `unusableReason`. The RPCs the
 * form issues go to the in-memory ConnectRPC backend, so the tests assert on the typed request the
 * component sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md.
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

/** The daemon that reaches the workspaces — their parent. */
const PARENT_DAEMON = "workstation-1";
/** The person's Coder account, as the accounts listing names it. */
const WORK_ACCOUNT = "coder-3f1c9a52@coder.example.com";
/** The running workspace's id: its parent, its kind, its account, its owner, its name. */
const DEVBOX = `${PARENT_DAEMON}/coder/${WORK_ACCOUNT}/ada/devbox`;
/** The stopped workspace's id. */
const SCRATCH = `${PARENT_DAEMON}/coder/${WORK_ACCOUNT}/ada/scratch`;
/** Why the stopped workspace cannot be used, as its parent says it. */
const STOPPED = "the workspace is stopped";

/** `ada`'s workspace `workspace` of the Work account, as a start addresses it. */
function theWorkspaceTarget(workspace: string) {
  return create(SessionTargetSchema, {
    kind: { case: "coder", value: { accountId: WORK_ACCOUNT, owner: "ada", workspace } },
  });
}

/** One daemon, and the two workspaces it reaches, as the host directory lists them. */
const HOSTS: DaemonHost[] = [
  { instanceId: PARENT_DAEMON, label: "workstation-1 (this daemon)" },
  {
    instanceId: DEVBOX,
    label: "devbox (Work)",
    parent: PARENT_DAEMON,
    target: theWorkspaceTarget("devbox"),
    usable: true,
  },
  {
    instanceId: SCRATCH,
    label: "scratch (Work)",
    parent: PARENT_DAEMON,
    target: theWorkspaceTarget("scratch"),
    usable: false,
    unusableReason: STOPPED,
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
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "coder-host-1" }));
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

it("starts on the parent daemon with the workspace as the target", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);

  // When — fill the required fields, pick the running workspace, and create
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent(CLAUDE_OPTION);
  createSessionPage.selectHost(DEVBOX);
  createSessionPage.submit();

  // Then — StartSession goes to the daemon that reaches the workspace, naming the account and the
  // workspace as its target
  cy.wrap(null).should(() => {
    const starts = backend.callsTo(SessionService.method.startSession).map((start) => ({
      daemonInstanceId: start.daemonInstanceId,
      targetKind: start.target?.kind.case,
      target: start.target?.kind.value,
    }));
    expect(starts).to.have.length(1);
    expect(starts[0].daemonInstanceId).to.equal(PARENT_DAEMON);
    expect(starts[0].targetKind).to.equal("coder");
    expect(starts[0].target).to.include({
      accountId: WORK_ACCOUNT,
      owner: "ada",
      workspace: "devbox",
    });
  });
});

it("does not let a stopped workspace be chosen", () => {
  // Given a new-session form over one daemon, a running workspace and a stopped one
  mountCreatePane(aCreateSessionBackend());

  // Then both workspaces are listed under the daemon that reaches them — the stopped one is not
  // hidden
  createSessionPage
    .derivedHostOptions(PARENT_DAEMON)
    .should("deep.equal", [
      [DEVBOX, "devbox (Work)"],
      [SCRATCH, `scratch (Work) — ${STOPPED}`],
    ]);
  // And only the stopped one cannot be chosen, its caption saying why
  createSessionPage
    .unusableDerivedHostOptions(PARENT_DAEMON)
    .should("deep.equal", [[SCRATCH, `scratch (Work) — ${STOPPED}`]]);
});
