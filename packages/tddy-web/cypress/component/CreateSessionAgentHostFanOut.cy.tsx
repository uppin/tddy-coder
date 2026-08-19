/**
 * Acceptance: the new-session form's **Agent** `<select>` — the agent a tool session is started
 * *as* — lists the agents of **every** common-room daemon, names the host that offers each, and
 * sets the session's Host from the agent that was picked — except in the peer-agent spawn flow,
 * where the host is settled before the form opens and only that host's agents may be offered.
 *
 * Feature: docs/ft/web/1-WIP/PRD-2026-08-19-session-agent-host-fan-out.md (AC1–AC12)
 *
 * `ListAgents` carries no routing field, so a daemon answers for its own config allowlist and its
 * own registry assistants only. Each host therefore answers from its **own** backend
 * (`mountWithPerDaemonLiveKitRpc`), and an option appearing under host B is proof the fan-out
 * reached B — not proof of a fixture.
 */

import React from "react";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectionService } from "../../src/gen/connection_pb";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { daemonRpcIdentity, type DaemonHost } from "../../src/lib/participantRole";
import { mountWithPerDaemonLiveKitRpc } from "../support/rpc/perDaemonLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { createSessionPage as page } from "../support/pages/createSessionPage";
import { recordedFields } from "../support/rpc/recordedRequests";

/** Host A — the daemon the form is pointed at, and the default selection. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** Host B — a peer, never the selected daemon. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

const CLAUDE_ON_A = "claude@workstation-1";
const CLAUDE_ON_B = "claude@server-2";
/** A registry assistant — a provider, a model, a system prompt and an assigned tool set. */
const REVIEWER_ON_B = "reviewer@server-2";

/** One row of a host's `ListAgents` answer. */
interface AgentRow {
  readonly id: string;
  readonly label: string;
}

const CLAUDE: AgentRow = { id: "claude", label: "Claude" };
const REVIEWER: AgentRow = { id: "reviewer", label: "Reviewer" };

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/**
 * The form's own RPCs, answered by whichever daemon the pane's `client` addresses. One project and
 * one tool path so the only field an operator still has to choose is the agent — which is what these
 * tests are about.
 */
function aCreateSessionBackend(agents: readonly AgentRow[]) {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-5", label: "Claude Opus 5" }],
      defaultModel: "claude-opus-5",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [...agents] }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "new-1" }));
}

/** A peer host that answers `ListAgents` with exactly `agents`. */
function aHostOffering(agents: readonly AgentRow[]): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listAgents, () => ({ agents: [...agents] }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }));
}

/** A peer host whose agent catalog cannot be read at all. */
function aHostThatCannotBeReached(message: string): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listAgents, () => {
      throw new ConnectError(message, Code.Unavailable);
    })
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: [] }));
}

/**
 * Mount the form pointed at host A, with each host answering its own `ListAgents`.
 *
 * `hosts` is the daemon list the common room advertises; the default is both hosts, and `[]` stands
 * for a browser with no common room at all.
 */
function mountForm(
  hostAAgents: readonly AgentRow[],
  hostB: InMemoryRpcBackend,
  hosts: DaemonHost[] = [HOST_A, HOST_B],
): InMemoryRpcBackend {
  const hostA = aCreateSessionBackend(hostAAgents);
  mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(
      <CreateSessionPane
        client={createClient(ConnectionService, hostA.transport())}
        sessionToken="tok"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />,
      hosts,
    ),
    {
      [daemonRpcIdentity(HOST_A.instanceId)]: hostA,
      [daemonRpcIdentity(HOST_B.instanceId)]: hostB,
    },
    { httpBackend: hostA },
  );
  return hostA;
}

/**
 * Mount the form as the **peer-agent spawn** flow: a second agent joining an existing session's
 * worktree on `orchestratorHost`. The pane's own `client` still addresses host A — the app-level
 * selected daemon — but the session being joined decides the host, so that is the host whose agents
 * the form may offer.
 */
function mountPeerForm(
  orchestratorHost: DaemonHost,
  hostAAgents: readonly AgentRow[],
  hostB: InMemoryRpcBackend,
): InMemoryRpcBackend {
  const hostA = aCreateSessionBackend(hostAAgents);
  mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(
      <CreateSessionPane
        client={createClient(ConnectionService, hostA.transport())}
        sessionToken="tok"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
        peerMode
        initialValues={{
          sessionType: "tool",
          projectId: "proj-1",
          daemonInstanceId: orchestratorHost.instanceId,
          repoPath: "/repo/.worktrees/orchestrator",
        }}
      />,
      [HOST_A, HOST_B],
    ),
    {
      [daemonRpcIdentity(HOST_A.instanceId)]: hostA,
      [daemonRpcIdentity(HOST_B.instanceId)]: hostB,
    },
    { httpBackend: hostA },
  );
  return hostA;
}

/** The `(agent, daemon_instance_id)` pair carried by every `StartSession` host A received. */
function startedSessionAgents(hostA: InMemoryRpcBackend): Array<[string, string]> {
  return recordedFields(hostA.callsTo(ConnectionService.method.startSession)).map((req) => {
    const fields = req as { agent: string; daemonInstanceId: string };
    return [fields.agent, fields.daemonInstanceId];
  });
}

describe("CreateSession Agent select across hosts", () => {
  beforeEach(() => {
    cy.viewport(1280, 900);
  });

  // -------------------------------------------------------------------------
  // AC1, AC2, AC3, AC11 — every host's agents, labelled and distinguishable
  // -------------------------------------------------------------------------

  it("lists the agents of every connected host, labelled by the host that offers them", () => {
    // Given — the same agent id on two hosts, plus an assistant only host B has
    mountForm([CLAUDE], aHostOffering([CLAUDE, REVIEWER]));

    // Then
    page.agentOption(CLAUDE_ON_A).should("have.text", "Claude · workstation-1");
    page.agentOption(CLAUDE_ON_B).should("have.text", "Claude · server-2");
    page.agentOption(REVIEWER_ON_B).should("have.text", "Reviewer · server-2");
  });

  it("offers an agent of the same id on two hosts as two separate choices", () => {
    // Given
    mountForm([CLAUDE], aHostOffering([CLAUDE, REVIEWER]));

    // Then — one option per (agent, host) pair, the home host's first
    page.agentOption(CLAUDE_ON_B).should("exist");
    page.agentOptionValues().should("deep.equal", [CLAUDE_ON_A, CLAUDE_ON_B, REVIEWER_ON_B]);
  });

  it("offers a peer host's assistant as an agent a session can start as", () => {
    // Given — the case the fan-out exists for: an assistant created on another host
    mountForm([CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // When
    page.selectAgent(REVIEWER_ON_B);

    // Then
    page.selectedAgentValue().should("equal", REVIEWER_ON_B);
  });

  // -------------------------------------------------------------------------
  // AC4, AC5 — the picked agent decides the host, and the request says so
  // -------------------------------------------------------------------------

  it("sets the session host to the host of the agent that was picked", () => {
    // Given
    mountForm([CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // When
    page.selectAgent(REVIEWER_ON_B);

    // Then
    page.hostSelect().should("have.value", HOST_B.instanceId);
  });

  it("starts the session with the bare agent id and the agent's host", () => {
    // Given
    const hostA = mountForm([CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // When
    page.selectAgent(REVIEWER_ON_B);
    page.submitButton().should("not.be.disabled").click();

    // Then — the wire format is unchanged: a bare id, and the host beside it
    cy.wrap(null).should(() => {
      expect(startedSessionAgents(hostA)).to.deep.equal([["reviewer", HOST_B.instanceId]]);
    });
  });

  // -------------------------------------------------------------------------
  // AC6, AC7 — an unlistable host, and nothing on offer at all
  // -------------------------------------------------------------------------

  it("costs one row when a host cannot be listed rather than the whole select", () => {
    // Given
    mountForm([CLAUDE], aHostThatCannotBeReached("server-2 is not reachable"));

    // Then — host A's agent is still offered, and host B is one visible error
    page.agentHostError(HOST_B.instanceId).should("contain.text", "server-2 is not reachable");
    page.agentOption(CLAUDE_ON_A).should("exist");
    page.agentOptionValues().should("deep.equal", [CLAUDE_ON_A]);
  });

  it("offers a disabled placeholder when no host has an agent", () => {
    // Given — both hosts answer, and neither offers anything
    mountForm([], aHostOffering([]));

    // Then
    page.agentEmptyOption().should("have.text", "No agents available");
    page.agentEmptyOption().should("be.disabled");
  });

  // -------------------------------------------------------------------------
  // AC8, AC9 — the selection and the host stay consistent with each other
  // -------------------------------------------------------------------------

  it("opens on the home host's first agent so opening the form does not move the host", () => {
    // Given — host B answers too, so its agents are on offer but must not be preselected
    mountForm([CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // Then
    page.selectedAgentValue().should("equal", CLAUDE_ON_A);
    page.hostSelect().should("have.value", HOST_A.instanceId);
  });

  it("keeps the selected agent when the new host offers one of the same name", () => {
    // Given
    mountForm([CLAUDE], aHostOffering([CLAUDE]));
    page.agentOption(CLAUDE_ON_B).should("exist");

    // When — only the host is changed
    page.selectHost(HOST_B.instanceId);

    // Then — the same agent, now the one that host serves
    page.selectedAgentValue().should("equal", CLAUDE_ON_B);
  });

  it("selects the new host's first agent when it does not offer the selected one", () => {
    // Given
    mountForm([CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");
    page.selectedAgentValue().should("equal", CLAUDE_ON_A);

    // When
    page.selectHost(HOST_B.instanceId);

    // Then
    page.selectedAgentValue().should("equal", REVIEWER_ON_B);
  });

  // -------------------------------------------------------------------------
  // AC10 — one host, nothing to disambiguate
  // -------------------------------------------------------------------------

  it("leaves option values bare when no daemons are advertised", () => {
    // Given — no common room, so there is one host and no host to name
    mountForm([CLAUDE], aHostOffering([REVIEWER]), []);

    // Then
    page.agentOption("claude").should("have.text", "Claude");
    page.agentOptionValues().should("deep.equal", ["claude"]);
    page.hostSelect({ timeout: 0 }).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC12 — peer mode: the host is decided before the form opens
  // -------------------------------------------------------------------------

  it("offers only the agents of the host the peer will run on", () => {
    // Given — a peer joining a session on host B, while the form itself addresses host A
    mountPeerForm(HOST_B, [CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // Then — host A's agent is not on offer: host B could not resolve it, and the peer runs there
    page.agentOptionValues().should("deep.equal", [REVIEWER_ON_B]);
  });

  it("starts a peer as an agent the host it runs on offers", () => {
    // Given
    const hostA = mountPeerForm(HOST_B, [CLAUDE], aHostOffering([REVIEWER]));
    page.agentOption(REVIEWER_ON_B).should("exist");

    // When
    page.submitButton().should("not.be.disabled").click();

    // Then — an agent and a host that agree, so the host can resolve what it is asked to run
    cy.wrap(null).should(() => {
      expect(startedSessionAgents(hostA)).to.deep.equal([["reviewer", HOST_B.instanceId]]);
    });
  });

  it("stays silent about a host the peer will not run on failing to answer", () => {
    // Given — the peer runs on host A; host B cannot be listed and has no bearing on it
    mountPeerForm(HOST_A, [CLAUDE], aHostThatCannotBeReached("server-2 is not reachable"));
    page.agentOption(CLAUDE_ON_A).should("exist");

    // Then
    page.agentHostError(HOST_B.instanceId, { timeout: 0 }).should("not.exist");
  });
});
