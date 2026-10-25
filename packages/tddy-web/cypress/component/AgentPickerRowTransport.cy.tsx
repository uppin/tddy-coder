/**
 * Acceptance: each coding-agent row in the agent picker names its transport and its permission
 * policy as text, so the policy is never implied by the agent being a coding agent.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § Coding agents
 *
 * The picker is mounted on its own, served by one host, so what is under test is the row's own
 * text — not the pane or the form the picker sits in.
 */

import React from "react";
import { AgentPicker } from "../../src/components/sessions/AgentPicker";
import type { DaemonHost } from "../../src/lib/participantRole";
import { withSelectedDaemonServedBy } from "../support/rpc/withSelectedDaemon";
import {
  aDaemonOfferingAgents,
  anAvailableCodingAgent,
} from "../support/rpc/sessionAgentRosterBackend";
import { agentRosterPickerOption, byTestId } from "../support/testIds";

/** The host that serves the picker and offers both coding agents. */
const HOST: DaemonHost = { instanceId: "workstation-1", label: "workstation-1 (this daemon)" };

const REVIEWER_ID = "reviewer@workstation-1";
const AUDITOR_ID = "auditor@workstation-1";

const HEADLESS_POLICY = "fixed: read-only; the CLI cannot ask mid-turn";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** A coding agent this host offers, carrying the transport and policy the daemon reports for it. */
function aCodingAgent(name: string, harness: string, transport: string, permissionPolicy: string) {
  return {
    ...anAvailableCodingAgent(name, HOST.instanceId, harness),
    transport,
    permissionPolicy,
  };
}

/** An ACP coding agent that asks its caller, offered beside a headless coding agent. */
function theCodingAgentsOnOffer() {
  return [
    aCodingAgent("reviewer", "claude-acp", "acp", "asks_caller"),
    aCodingAgent("auditor", "cursor", "headless", HEADLESS_POLICY),
  ];
}

/** Mount the picker, served by {@link HOST}, whose catalog offers `agents`. */
function mountThePicker(agents: ReturnType<typeof aCodingAgent>[]) {
  cy.mountWithRpc(
    withSelectedDaemonServedBy(
      <AgentPicker
        testIdPrefix="agent-roster-picker"
        errorTestId="agent-roster-attach-error"
        onAttach={cy.stub()}
        onClose={cy.stub()}
      />,
      [HOST],
      HOST.instanceId,
    ),
    aDaemonOfferingAgents(agents),
  );
}

/**
 * The line a coding-agent row carries beneath its name: its transport, then its permission policy.
 * Its id is the option's own id with a suffix, as the row's host and kind labels are.
 */
function transportAndPolicyOf(agentId: string) {
  return byTestId(`${agentRosterPickerOption(agentId)}-transport-policy`);
}

describe("Agent picker coding-agent rows name their transport and permission policy", () => {
  it("an ACP coding agent's row says its transport and that it asks its caller", () => {
    // Given — an ACP coding agent that asks its caller, offered beside a headless coding agent
    const agents = theCodingAgentsOnOffer();

    // When
    mountThePicker(agents);

    // Then
    transportAndPolicyOf(REVIEWER_ID)
      .should("contain.text", "acp")
      .and("contain.text", "asks_caller");
  });

  it("a headless coding agent's row names its fixed permission policy in full", () => {
    // Given — a headless coding agent whose CLI cannot ask mid-turn, offered beside an ACP one
    const agents = theCodingAgentsOnOffer();

    // When
    mountThePicker(agents);

    // Then
    transportAndPolicyOf(AUDITOR_ID)
      .should("contain.text", "headless")
      .and("contain.text", HEADLESS_POLICY);
  });
});
