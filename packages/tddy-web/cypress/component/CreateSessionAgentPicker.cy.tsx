/**
 * Acceptance: the new-session form's specialized-agent picker lists agents from **every**
 * common-room daemon, labels each with the host that offers it, sends **qualified** ids, and
 * degrades one unreachable host into one error row rather than an empty picker.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md (AC48, AC49)
 * Changeset: docs/dev/1-WIP/2026-08-16-session-agent-roster.md
 *
 * Supersedes the single-daemon `listSubagents` stub in `CreateSessionManagedCodebase.cy.tsx`: that
 * picker asked one daemon and sent bare names, which cannot express an agent that lives elsewhere.
 *
 * Each host answers from its **own** backend (`mountWithPerDaemonLiveKitRpc`), so an option
 * appearing under host B is proof the fan-out reached B — not proof of a fixture.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService } from "../../src/gen/connection_pb";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { daemonRpcIdentity, type DaemonHost } from "../../src/lib/participantRole";
import { mountWithPerDaemonLiveKitRpc } from "../support/rpc/perDaemonLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import {
  aDaemonOfferingAgents,
  aDaemonThatCannotBeReached,
  anAvailableAgent,
} from "../support/rpc/sessionAgentRosterBackend";
import {
  TEST_IDS,
  byTestId,
  createSessionAgentHostError,
  createSessionAgentOption,
  createSessionAgentOptionHost,
} from "../support/testIds";
import { recordedFields } from "../support/rpc/recordedRequests";

/** Host A — the daemon the form is pointed at. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** Host B — a peer, never the selected daemon. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

const EXPLORER_ON_A = "explorer@workstation-1";
const EXPLORER_ON_B = "explorer@server-2";
const LINTER_ON_B = "linter@server-2";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The RPCs `CreateSessionPane` calls on mount, on whichever daemon it is pointed at. */
function aCreateSessionBackend() {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-5", label: "Claude Opus 5" }],
      defaultModel: "claude-opus-5",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [] }))
    .onUnary(ConnectionService.method.listTools, () => ({ tools: [] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "new-1" }));
}

/**
 * Mount the create pane with host A serving the form's own RPCs and each host serving its own
 * `ListSubagents`.
 */
function mountPicker(hostB: InMemoryRpcBackend): InMemoryRpcBackend {
  const hostABackend = aCreateSessionBackend().onUnary(
    ConnectionService.method.listSubagents,
    () => ({ subagents: [anAvailableAgent("explorer", HOST_A.instanceId, ["Grep"])] }),
  );
  mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(
      <CreateSessionPane
        client={createClient(ConnectionService, hostABackend.transport())}
        sessionToken="tok"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />,
      [HOST_A, HOST_B],
    ),
    {
      [daemonRpcIdentity(HOST_A.instanceId)]: hostABackend,
      [daemonRpcIdentity(HOST_B.instanceId)]: hostB,
    },
    { httpBackend: hostABackend },
  );
  return hostABackend;
}

/** Put the form into the state where the agent picker is rendered. */
function openTheAgentPicker() {
  byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
  byTestId(TEST_IDS.createSessionManagedCodebaseToggle).click();
}

/** The `specialized_agents` lists carried by every `StartSession` host A received. */
function startedSessionAgentLists(hostA: InMemoryRpcBackend): string[][] {
  return recordedFields(hostA.callsTo(ConnectionService.method.startSession)).map(
    (req) => (req as { specializedAgents: string[] }).specializedAgents,
  );
}

describe("CreateSession specialized-agent picker across hosts", () => {
  beforeEach(() => {
    cy.viewport(1280, 900);
  });

  // -------------------------------------------------------------------------
  // AC48 — every host's agents, labelled and qualified
  // -------------------------------------------------------------------------

  it("lists agents from every connected host labelled by the host that offers them", () => {
    // Given — the same agent name on two hosts, which is the case a bare name cannot express
    mountPicker(
      aDaemonOfferingAgents([
        anAvailableAgent("explorer", HOST_B.instanceId, ["Grep"]),
        anAvailableAgent("linter", HOST_B.instanceId),
      ]),
    );

    // When
    openTheAgentPicker();

    // Then
    byTestId(createSessionAgentOption(EXPLORER_ON_A)).should("exist");
    byTestId(createSessionAgentOption(EXPLORER_ON_B)).should("exist");
    byTestId(createSessionAgentOption(LINTER_ON_B)).should("exist");
    byTestId(createSessionAgentOptionHost(EXPLORER_ON_A)).should("have.text", HOST_A.instanceId);
    byTestId(createSessionAgentOptionHost(EXPLORER_ON_B)).should("have.text", HOST_B.instanceId);
  });

  it("sends the qualified id of every agent the operator picked", () => {
    // Given
    const hostA = mountPicker(
      aDaemonOfferingAgents([anAvailableAgent("linter", HOST_B.instanceId)]),
    );
    openTheAgentPicker();

    // When — one agent from each host
    byTestId(createSessionAgentOption(EXPLORER_ON_A)).click();
    byTestId(createSessionAgentOption(LINTER_ON_B)).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    // Then
    cy.wrap(null).should(() => {
      expect(startedSessionAgentLists(hostA)).to.deep.equal([[EXPLORER_ON_A, LINTER_ON_B]]);
    });
  });

  it("sends no agents when the managed-codebase section is left closed", () => {
    // Given
    const hostA = mountPicker(
      aDaemonOfferingAgents([anAvailableAgent("linter", HOST_B.instanceId)]),
    );

    // When — claude-cli, but the section is never opened
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    // Then
    cy.wrap(null).should(() => {
      expect(startedSessionAgentLists(hostA)).to.deep.equal([[]]);
    });
  });

  // -------------------------------------------------------------------------
  // AC49 — one host's failure costs one row
  // -------------------------------------------------------------------------

  it("costs one row when a host cannot answer rather than the whole picker", () => {
    // Given
    mountPicker(aDaemonThatCannotBeReached("server-2 is not reachable"));

    // When
    openTheAgentPicker();

    // Then — host A's agent is still offered, and host B is one visible error
    byTestId(createSessionAgentOption(EXPLORER_ON_A)).should("exist");
    byTestId(createSessionAgentHostError(HOST_B.instanceId)).should(
      "contain.text",
      "server-2 is not reachable",
    );
  });

  it("still starts a session when one host could not be listed", () => {
    // Given
    const hostA = mountPicker(aDaemonThatCannotBeReached("server-2 is not reachable"));
    openTheAgentPicker();

    // When
    byTestId(createSessionAgentOption(EXPLORER_ON_A)).click();
    byTestId(TEST_IDS.createSessionSubmitBtn).click();

    // Then
    cy.wrap(null).should(() => {
      expect(startedSessionAgentLists(hostA)).to.deep.equal([[EXPLORER_ON_A]]);
    });
  });
});
