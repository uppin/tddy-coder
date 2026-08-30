/**
 * Acceptance: the Agent roster pane — the **managed** agents attached to a session, with an Add
 * flow that says what the main agent loses and a Detach flow that confirms before deleting a
 * checkout on another host.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md (AC49-AC53)
 *
 * Scoped to the roster half of the pane. The tree it now renders into — the main agent at the root
 * and the subagent *sessions* beneath it — is `SessionAgentSubagentTree.cy.tsx`'s subject; the two
 * populations share the word "agent" and nothing else.
 *
 * Roster frames are pushed by the in-memory backend rather than stubbed once, because the property
 * under test is that the pane follows a roster changed *elsewhere*: a `cy.intercept` snapshot could
 * never distinguish "rendered the roster it was mounted with" from "followed the stream".
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  AgentCloneState,
  ConnectionService,
  SessionEntrySchema,
} from "../../src/gen/connection_pb";
import { SessionAgentRosterPane } from "../../src/components/sessions/SessionAgentRosterPane";
import { daemonRpcIdentity, type DaemonHost } from "../../src/lib/participantRole";
import {
  aDaemonOfferingAgents,
  aDaemonThatCannotBeReached,
  aRemoteAttachedAgent,
  aSessionAgentRosterBackend,
  anAttachedAgent,
  anAvailableAgent,
  type RosterBackend,
} from "../support/rpc/sessionAgentRosterBackend";
import { mountWithPerDaemonLiveKitRpc } from "../support/rpc/perDaemonLiveKitRpc";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";

const SESSION_ID = "1780828020298-roster";
const EXPLORER_LOCAL = "explorer@workstation-1";
const LINTER_REMOTE = "linter@server-2";

/** The host facilitating the session — it owns the roster and answers the picker for its own defs. */
const HOST_A: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };
/** A peer host in the common room, reached only by the picker's fan-out. */
const HOST_B: DaemonHost = { instanceId: "server-2", label: "server-2" };

/**
 * The session the pane is mounted for. Co-located, so the roster half it addresses is the session
 * itself — the split case is `SessionAgentRosterSplitSession.cy.tsx`.
 */
const SESSION = create(SessionEntrySchema, {
  sessionId: SESSION_ID,
  createdAt: "2026-08-29T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 90001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: HOST_A.instanceId,
  sessionType: "claude-cli",
  agent: "claude",
  model: "opus-4",
});

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/**
 * Publish a roster revision **in command-queue order**.
 *
 * A bare `roster.pushRoster(...)` is a plain function call, so it would run while the test body is
 * being evaluated — before `cy.mount` has even been dequeued. The revision would then already be in
 * the fake's tail when the stream subscribes, arrive folded into the first frame, and never be a
 * live change at all. Wrapping it in `cy.then` puts it where the `// When` it sits under says it is;
 * this mirrors `deliver` in `LiveKitRoomsPanelAcceptance.cy.tsx`, which has the same pushable tail.
 */
function publish(roster: RosterBackend, agents: Parameters<typeof roster.pushRoster>[0], rev: number) {
  cy.then(() => roster.pushRoster(agents, rev));
}

function mountPane(roster: RosterBackend, options: { connected?: boolean } = {}) {
  cy.mountWithRpc(
    <SessionAgentRosterPane
      session={SESSION}
      sessions={[SESSION]}
      sessionToken="tok"
      daemonConnected={options.connected ?? true}
      onSwitchSubagent={cy.stub().as("switchSubagent")}
    />,
    roster.backend,
  );
}

/**
 * Mount the pane in a two-host common room, with each host answering the picker's `ListSubagents`
 * from its **own** backend. An option — or an error row — attributed to host B is therefore proof
 * the fan-out reached host B, not proof of a fixture; this mirrors `CreateSessionAgentPicker.cy.tsx`.
 */
function mountPaneAcrossHosts(roster: RosterBackend, hostB: InMemoryRpcBackend) {
  mountWithPerDaemonLiveKitRpc(
    withSelectedDaemon(
      <SessionAgentRosterPane
        session={SESSION}
        sessions={[SESSION]}
        sessionToken="tok"
        daemonConnected
        onSwitchSubagent={cy.stub().as("switchSubagent")}
      />,
      [HOST_A, HOST_B],
    ),
    { [daemonRpcIdentity(HOST_B.instanceId)]: hostB },
    { httpBackend: roster.backend },
  );
}

/**
 * How many times a backend was asked for its agent catalog.
 *
 * The pane's own client and the common room are two ways of naming *one* host, and `ListSubagents`
 * is not free: it is what the picker's fan-out asks every host for. Counting the reads one backend
 * received is the only way to state "asked once" — the answers of a host asked twice are identical,
 * so nothing on screen distinguishes one read from two.
 */
function catalogReadsReceivedBy(roster: RosterBackend) {
  return cy.then(() => roster.backend.callsTo(ConnectionService.method.listSubagents));
}

describe("Agent roster pane", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  // -------------------------------------------------------------------------
  // AC50 — the pane follows the roster
  // -------------------------------------------------------------------------

  it("shows an agent attached from somewhere else without being asked to refresh", () => {
    // Given — a session with one agent attached
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [anAttachedAgent(EXPLORER_LOCAL)],
      rev: 1,
    });
    mountPane(roster);
    page.row(EXPLORER_LOCAL).should("exist");

    // When — another operator attaches a second agent
    publish(roster, [anAttachedAgent(EXPLORER_LOCAL), aRemoteAttachedAgent(LINTER_REMOTE)], 2);

    // Then
    page.row(LINTER_REMOTE).should("exist");
  });

  it("drops an agent detached from somewhere else", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [anAttachedAgent(EXPLORER_LOCAL), aRemoteAttachedAgent(LINTER_REMOTE)],
      rev: 2,
    });
    mountPane(roster);
    page.row(LINTER_REMOTE).should("exist");

    // When
    publish(roster, [anAttachedAgent(EXPLORER_LOCAL)], 3);

    // Then
    page.row(LINTER_REMOTE).should("not.exist");
    page.row(EXPLORER_LOCAL).should("exist");
  });

  it("names the host each attached agent belongs to", () => {
    // Given — two agents with the same name on different hosts, which is the ordinary case
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [
        anAttachedAgent("explorer@workstation-1"),
        aRemoteAttachedAgent("explorer@server-2"),
      ],
      rev: 2,
    });

    // When
    mountPane(roster);

    // Then
    page.rowHost("explorer@workstation-1").should("have.text", "workstation-1");
    page.rowHost("explorer@server-2").should("have.text", "server-2");
  });

  it("shows the tools an attached agent takes away from the main agent", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [anAttachedAgent(EXPLORER_LOCAL, { replaces: ["Grep", "Glob"] })],
      rev: 1,
    });

    // When
    mountPane(roster);

    // Then
    page.rowReplaces(EXPLORER_LOCAL).should("contain.text", "Grep");
    page.rowReplaces(EXPLORER_LOCAL).should("contain.text", "Glob");
  });

  it("shows a remote agent's clone as provisioning until it is ready", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [
        aRemoteAttachedAgent(LINTER_REMOTE, { cloneState: AgentCloneState.PROVISIONING }),
      ],
      rev: 1,
    });
    mountPane(roster);
    page
      .rowCloneState(LINTER_REMOTE)
      .should("have.attr", "data-clone-state", "provisioning");

    // When
    publish(roster, [aRemoteAttachedAgent(LINTER_REMOTE)], 2);

    // Then
    page.rowCloneState(LINTER_REMOTE).should("have.attr", "data-clone-state", "ready");
  });

  // -------------------------------------------------------------------------
  // AC51 — the add flow states the cost
  // -------------------------------------------------------------------------

  it("says which tools the main agent loses before the operator confirms", () => {
    // Given — the daemon offers an agent that would withdraw Grep and Glob
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", "workstation-1", ["Grep", "Glob"])],
    });
    mountPane(roster);

    // When
    page.openPicker();
    page.selectInPicker(EXPLORER_LOCAL);

    // Then
    page.pickerWithdrawalWarning().should("contain.text", "Grep");
    page.pickerWithdrawalWarning().should("contain.text", "Glob");
  });

  it("attaches the agent under its qualified id when the operator confirms", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", "workstation-1", ["Grep", "Glob"])],
    });
    mountPane(roster);

    // When
    page.openPicker();
    page.selectInPicker(EXPLORER_LOCAL);
    page.confirmAttach();

    // Then — the attach is sent under the qualified id, and the pane shows the roster the daemon
    // published for it rather than only the call it made
    cy.wrap(null).should(() => {
      expect(roster.attachedAgentIds()).to.deep.equal([EXPLORER_LOCAL]);
    });
    page.row(EXPLORER_LOCAL).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC49 in the roster's own picker — the same fan-out the create-session picker makes, so the same
  // failure has to cost the same: one row, never the picker.
  // -------------------------------------------------------------------------

  it("keeps the reachable host's agents on offer when another host cannot answer, at the cost of one row", () => {
    // Given — the session's host offers an agent; the peer cannot answer the fan-out at all
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", HOST_A.instanceId, ["Grep"])],
    });
    mountPaneAcrossHosts(roster, aDaemonThatCannotBeReached("server-2 is not reachable"));

    // When
    page.openPicker();

    // Then — host A's agent is still on offer, and host B costs exactly one error row
    page.pickerOption(EXPLORER_LOCAL).should("exist");
    page.pickerOptionHost(EXPLORER_LOCAL).should("have.text", HOST_A.instanceId);
    page.pickerHostError(HOST_B.instanceId).should("contain.text", "server-2 is not reachable");
  });

  it("asks the host behind its own client once, however many ways that host can be named", () => {
    // Given — a two-host common room in which the bundle's own host is not named, so the id the room
    // advertises for it is the only thing that can tell the pane's client apart from a peer
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      rev: 0,
      offers: [anAvailableAgent("explorer", HOST_A.instanceId, ["Grep"])],
    });
    mountPaneAcrossHosts(
      roster,
      aDaemonOfferingAgents([anAvailableAgent("linter", HOST_B.instanceId, ["Read"])]),
    );

    // When — the picker opens, which is what fans the catalog read out across the room
    page.openPicker();

    // Then — both hosts answered, and the session's host was asked for its catalog once
    page.pickerOption(EXPLORER_LOCAL).should("exist");
    page.pickerOption(LINTER_REMOTE).should("exist");
    catalogReadsReceivedBy(roster).should("have.length", 1);
  });

  // -------------------------------------------------------------------------
  // AC52 — detaching a remote agent deletes a checkout
  // -------------------------------------------------------------------------

  it("asks before a detach that deletes a checkout on another host", () => {
    // Given — the only agent owned by server-2
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [anAttachedAgent(EXPLORER_LOCAL), aRemoteAttachedAgent(LINTER_REMOTE)],
      rev: 2,
    });
    mountPane(roster);

    // When
    page.clickDetach(LINTER_REMOTE);

    // Then — nothing is sent until the operator confirms
    page.detachConfirmation().should("contain.text", "server-2");
    cy.wrap(null).should(() => {
      expect(roster.detachedAgentIds()).to.deep.equal([]);
    });

    page.confirmDetach();
    cy.wrap(null).should(() => {
      expect(roster.detachedAgentIds()).to.deep.equal([LINTER_REMOTE]);
    });
  });

  it("detaches a local agent without asking, because no checkout is destroyed", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [anAttachedAgent(EXPLORER_LOCAL)],
      rev: 1,
    });
    mountPane(roster);

    // When
    page.clickDetach(EXPLORER_LOCAL);

    // Then — nothing is asked, the detach is sent, and the row goes with the revision that follows
    page.detachConfirmation().should("not.exist");
    cy.wrap(null).should(() => {
      expect(roster.detachedAgentIds()).to.deep.equal([EXPLORER_LOCAL]);
    });
    page.row(EXPLORER_LOCAL).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC53 — four states, never one blank panel
  // -------------------------------------------------------------------------

  it("shows a disconnected host as disconnected rather than as an empty roster", () => {
    // Given
    const roster = aSessionAgentRosterBackend({ sessionId: SESSION_ID, initial: [], rev: 0 });

    // When
    mountPane(roster, { connected: false });

    // Then
    page.disconnected().should("be.visible");
    page.empty().should("not.exist");
    page.error().should("not.exist");
  });

  it("shows loading while the first roster frame has not arrived", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      neverAnswers: true,
    });

    // When
    mountPane(roster);

    // Then
    page.loading().should("be.visible");
    page.empty().should("not.exist");
  });

  it("shows a failed read as an error naming the failure, not as an empty roster", () => {
    // Given
    const roster = aSessionAgentRosterBackend({
      sessionId: SESSION_ID,
      initial: [],
      failBeforeSnapshot: "daemon is not reachable",
    });

    // When
    mountPane(roster);

    // Then
    page.error().should("contain.text", "daemon is not reachable");
    page.empty().should("not.exist");
  });

  it("shows a genuinely empty roster as empty", () => {
    // Given
    const roster = aSessionAgentRosterBackend({ sessionId: SESSION_ID, initial: [], rev: 0 });

    // When
    mountPane(roster);

    // Then
    page.empty().should("be.visible");
    page.error().should("not.exist");
    page.loading().should("not.exist");
  });
});
