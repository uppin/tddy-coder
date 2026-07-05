/**
 * Acceptance tests: the new-session form (CreateSessionPane) lets an operator choose which
 * daemon/host runs the session. The chosen host is carried on StartSession (and drives branch
 * listing) as `daemonInstanceId`, so a session can be created on a host other than the one the
 * form's client is connected to.
 *
 * The selectable hosts are the daemon-role participants in the common LiveKit room, supplied here
 * via a `SelectedDaemonProvider` fixture (mirrors `withSelectedDaemon`); the RPCs the form issues
 * are exercised against the in-memory ConnectRPC backend so the tests assert on the typed requests
 * the component actually sent.
 *
 * PRD: docs/ft/web/daemon-selector-livekit-rpc.md.
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
  { instanceId: REMOTE_HOST, label: "server-2 (this daemon)" },
];

/** A backend seeded with every RPC CreateSessionPane issues, plus StartSession + branch listing. */
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
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [{ id: "claude", label: "Claude" }] }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({ branches: ["origin/main"] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "host-sel-1" }));
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

it("lists every common-room daemon as a host option", () => {
  // Given a new-session form with two daemons in the common room
  mountCreatePane(aCreateSessionBackend());

  // When / Then — both hosts are offered as targets, in room order
  createSessionPage.hostOptionValues().should("deep.equal", [LOCAL_HOST, REMOTE_HOST]);
});

it("starts the session on the chosen host", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);

  // When — pick the remote host, fill the required fields, and create
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectAgent("claude");
  createSessionPage.selectHost(REMOTE_HOST);
  createSessionPage.submit();

  // Then — StartSession is routed to the chosen host, not the connected/local one
  cy.wrap(null).should(() => {
    const calls = backend.callsTo(ConnectionService.method.startSession);
    expect(calls).to.have.length(1);
    expect(calls[0].daemonInstanceId).to.equal(REMOTE_HOST);
  });
});

it("lists branches for the chosen host when working on an existing branch", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);

  // When — pick the remote host, then switch to working on an existing branch
  createSessionPage.selectProject("proj-1");
  createSessionPage.selectHost(REMOTE_HOST);
  createSessionPage.switchToWorkOnExistingBranch();

  // Then — branch listing is scoped to the chosen host
  cy.wrap(null).should(() => {
    const calls = backend.callsTo(ConnectionService.method.listProjectBranches);
    expect(calls).to.have.length.at.least(1);
    expect(calls[calls.length - 1].daemonInstanceId).to.equal(REMOTE_HOST);
  });
});
