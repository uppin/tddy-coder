/**
 * Acceptance tests: the new-session form lets an operator place a session's *codebase* on a
 * different daemon than the one running the agent. The agent then reaches the worktree only through
 * `mcp__tddy-tools__*` over LiveKit — it has no repository on its own host.
 *
 * The chosen daemon rides on `StartSession` as `codebaseDaemonInstanceId`. It is only meaningful
 * alongside `managedCodebase`, and only for claude-cli sessions, so the control is gated on both and
 * its value must never leak into a request that the daemon would refuse.
 *
 * Hosts come from the common LiveKit room via a `SelectedDaemonProvider` fixture; RPCs run against
 * the in-memory ConnectRPC backend so the tests assert on the typed request actually sent.
 *
 * PRD: docs/ft/daemon/remote-managed-worktree.md.
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

/** The daemon running the agent. */
const AGENT_HOST = "laptop-a";
/** The daemon holding the checkout — the one an operator would pick as the codebase host. */
const CODEBASE_HOST = "workstation-b";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: AGENT_HOST, label: "laptop-a (this daemon)" },
  { instanceId: CODEBASE_HOST, label: "workstation-b" },
];

/** "Same as host" — the co-located default, sent as an empty `codebaseDaemonInstanceId`. */
const SAME_AS_HOST = "";

function aCreateSessionBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "split-1" }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={AGENT_HOST}>
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />
    </SelectedDaemonProvider>,
  );
}

/** The single StartSession the form sent. */
function theStartSessionRequest(backend: InMemoryRpcBackend) {
  const calls = backend.callsTo(ConnectionService.method.startSession);
  expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
  return calls[0];
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests — availability of the control
// ---------------------------------------------------------------------------

it("offers the codebase host only once managed codebase is enabled", () => {
  // Given a claude-cli session with the managed-codebase section closed
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // Then — an agent that still holds native filesystem tools has nothing to proxy, so the
  // placement choice is not offered
  createSessionPage.expectNoCodebaseHostSelector();

  // When the section is opened
  createSessionPage.enableManagedCodebase();

  // Then it becomes available
  createSessionPage.codebaseHostSelect().should("be.visible");
});

it("offers the codebase host for claude-cli sessions but not for cursor-cli", () => {
  // Given a cursor-cli session with managed codebase enabled
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToCursorCliSession();
  createSessionPage.enableManagedCodebase();

  // Then — cursor-agent cannot enforce a tool allowlist, so split placement is claude-cli only
  createSessionPage.expectNoCodebaseHostSelector();

  // When the same managed session becomes a claude-cli one
  createSessionPage.switchToClaudeCliSession();

  // Then the choice appears — the gate is the session type, not the managed-codebase flag
  createSessionPage.codebaseHostSelect().should("be.visible");
});

it("offers same-as-host plus every common-room daemon as a codebase host", () => {
  // Given
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // When
  createSessionPage.enableManagedCodebase();

  // Then
  createSessionPage
    .codebaseHostOptionValues()
    .should("deep.equal", [SAME_AS_HOST, AGENT_HOST, CODEBASE_HOST]);
});

it("defaults the codebase host to same as host", () => {
  // Given
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // When
  createSessionPage.enableManagedCodebase();

  // Then — the default placement is co-located, matching every session created before this feature
  createSessionPage.codebaseHostSelect().should("have.value", SAME_AS_HOST);
  createSessionPage.codebaseHostOptionLabels().should("have.length.at.least", 1);
  createSessionPage
    .codebaseHostOptionLabels()
    .then((labels) => expect(labels[0]).to.equal("Same as host"));
});

// ---------------------------------------------------------------------------
// Tests — what reaches StartSession
// ---------------------------------------------------------------------------

it("sends the chosen codebase host on the start session request", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When — the worktree is placed on the workstation while the agent runs on the laptop
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.sessionType).to.equal("claude-cli");
    expect(request.managedCodebase).to.equal(true);
    expect(request.daemonInstanceId).to.equal(AGENT_HOST);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
  });
});

it("sends an empty codebase host when the codebase stays on the session host", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When — the operator leaves the placement alone
  createSessionPage.submit();

  // Then — a co-located request is indistinguishable from one sent before this feature existed
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});

it("drops a chosen codebase host when the session type changes to cursor-cli", () => {
  // Given a codebase host chosen on a claude-cli session
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // When — the operator switches to a session type that cannot be split
  createSessionPage.switchToCursorCliSession();
  createSessionPage.submit();

  // Then — the two managed-codebase blocks share state, so the stale value must not survive into a
  // request the daemon would refuse
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.sessionType).to.equal("cursor-cli");
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});

// ---------------------------------------------------------------------------
// Tests — a split session cannot carry a workflow recipe
// ---------------------------------------------------------------------------
//
// A recipe's tooling runs against a repository on the daemon hosting the agent, and a split
// session has none — the daemon refuses the combination outright. The form defaults `recipe` to
// "tdd" and sends it whenever managed codebase is on, so without this the *only* thing the codebase
// host selector could produce is a request the daemon rejects.

it("stops offering a workflow recipe once the codebase is placed on another host", () => {
  // Given a managed claude-cli session, which offers a recipe while the codebase is co-located
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.enableManagedCodebase();
  createSessionPage.recipeSelect().should("be.visible");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then the recipe is no longer offered, rather than offered and then silently refused
  createSessionPage.expectNoRecipeSelector();
});

it("sends no workflow recipe for a split session", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then — the form's default recipe must not ride along and turn a valid placement into a refusal
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
    expect(request.recipe).to.equal("");
  });
});

it("restores the workflow recipe when the codebase comes back to the session host", () => {
  // Given a split placement, with the recipe withdrawn
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.expectNoRecipeSelector();

  // When the operator puts the codebase back on the session's own host
  createSessionPage.selectCodebaseHost(SAME_AS_HOST);
  createSessionPage.submit();

  // Then the recipe returns — withdrawing it is a property of the split, not a one-way door
  createSessionPage.recipeSelect().should("be.visible");
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal("");
    expect(request.recipe).to.equal("tdd");
  });
});

it("drops a chosen codebase host when managed codebase is switched back off", () => {
  // Given a codebase host chosen inside an open managed-codebase section
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // When — the operator closes the section again
  createSessionPage.disableManagedCodebase();
  createSessionPage.submit();

  // Then — matching how specializedAgents and semanticIndex are cleared with the section
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.managedCodebase).to.equal(false);
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});
