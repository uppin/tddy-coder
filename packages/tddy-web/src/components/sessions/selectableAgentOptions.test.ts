/**
 * Unit coverage for the option/selection algebra behind the new-session form's **Agent** `<select>`,
 * whose options are fanned out across every common-room host.
 *
 * The form's observable behaviour — every host's agents listed and labelled, the Host following the
 * agent that was picked — is pinned by
 * `cypress/component/CreateSessionAgentHostFanOut.cy.tsx`. These specs pin the rules underneath it,
 * where the branching lives: how an option is keyed and captioned, and which agent a host change
 * lands on. A component spec can only reach those one full mount at a time.
 *
 * Feature doc: docs/ft/web/session-agent-catalog-fan-out.md
 */

import { describe, expect, it } from "bun:test";
import {
  agentForHost,
  hostRunningSession,
  selectableAgentText,
  selectableAgentValue,
  type SelectableAgent,
} from "./selectableAgentOptions";

const HOST_A = "workstation-1";
const HOST_B = "server-2";
const HOST_C = "server-3";

/** One agent as a host offers it. `label` is empty for a config agent the daemon captions by id. */
function aSelectableAgent(id: string, label: string, daemonInstanceId: string): SelectableAgent {
  return { id, label, daemonInstanceId };
}

describe("selectableAgentValue", () => {
  it("qualifies an option value with the host that offers the agent", () => {
    // Given — two hosts are advertised, so `claude` alone cannot say which one was picked
    const agent = aSelectableAgent("claude", "Claude", HOST_B);

    // When
    const value = selectableAgentValue(agent, true);

    // Then
    expect(value).toBe("claude@server-2");
  });

  it("leaves an option value bare when no host is advertised", () => {
    // Given — no common room: one host, and nothing to disambiguate
    const agent = aSelectableAgent("claude", "Claude", "");

    // When
    const value = selectableAgentValue(agent, false);

    // Then — the value every single-host caller has always submitted
    expect(value).toBe("claude");
  });
});

describe("selectableAgentText", () => {
  it("names the offering host in the option text", () => {
    // Given
    const agent = aSelectableAgent("reviewer", "Reviewer", HOST_B);

    // When
    const text = selectableAgentText(agent, true);

    // Then
    expect(text).toBe("Reviewer · server-2");
  });

  it("leaves the option text unqualified when no host is advertised", () => {
    // Given
    const agent = aSelectableAgent("reviewer", "Reviewer", "");

    // When
    const text = selectableAgentText(agent, false);

    // Then
    expect(text).toBe("Reviewer");
  });

  it("falls back to the id when an agent carries no label", () => {
    // Given — `AgentInfo.label` is optional, and the daemon leaves it empty for some rows
    const agent = aSelectableAgent("codex", "", HOST_B);

    // When
    const text = selectableAgentText(agent, true);

    // Then
    expect(text).toBe("codex · server-2");
  });
});

describe("hostRunningSession", () => {
  it("keeps the host the form asked for", () => {
    // Given — a session pinned to a named host, read from a browser connected to a different one
    // When
    const host = hostRunningSession(HOST_B, HOST_A);

    // Then
    expect(host).toBe(HOST_B);
  });

  it("names the connected daemon when the request names no host", () => {
    // Given — the spelling a peer inherits from an orchestrator started without an explicit host:
    // `daemon_instance_id: ""`, which the daemon serves on whichever host the request arrives at
    // When
    const host = hostRunningSession("", HOST_A);

    // Then — the host the request reaches anyway, so the agents offered are the ones it can resolve
    expect(host).toBe(HOST_A);
  });

  it("names no host when there is no daemon connection to name one from", () => {
    // Given — no common room: nothing is advertised, and the fan-out stamps its rows the same way
    // When
    const host = hostRunningSession("", "");

    // Then
    expect(host).toBe("");
  });
});

describe("agentForHost", () => {
  it("keeps the agent of the same name when the new host offers one", () => {
    // Given — both hosts offer `claude`, and `claude` is selected on host A
    const agents = [
      aSelectableAgent("claude", "Claude", HOST_A),
      aSelectableAgent("claude", "Claude", HOST_B),
    ];

    // When — the operator changes only the host
    const selected = agentForHost(agents, HOST_B, "claude");

    // Then — the same agent, now the one host B serves
    expect(selected).toEqual(aSelectableAgent("claude", "Claude", HOST_B));
  });

  it("falls to the new host's first agent when it does not offer the selected name", () => {
    // Given — host B has no `claude`
    const agents = [
      aSelectableAgent("claude", "Claude", HOST_A),
      aSelectableAgent("reviewer", "Reviewer", HOST_B),
      aSelectableAgent("cursor", "Cursor", HOST_B),
    ];

    // When
    const selected = agentForHost(agents, HOST_B, "claude");

    // Then — an agent host B can actually resolve, so the pair on screen is never contradictory
    expect(selected).toEqual(aSelectableAgent("reviewer", "Reviewer", HOST_B));
  });

  it("selects no agent when the new host offers none", () => {
    // Given — host B answered, and offers nothing
    const agents = [aSelectableAgent("claude", "Claude", HOST_A)];

    // When
    const selected = agentForHost(agents, HOST_B, "claude");

    // Then — absence is reported as absence, never as another host's agent
    expect(selected).toBeNull();
  });

  it("ignores an agent of the same name on a host that is not the new one", () => {
    // Given — `claude` exists on host A and host C, but not on host B
    const agents = [
      aSelectableAgent("claude", "Claude", HOST_A),
      aSelectableAgent("reviewer", "Reviewer", HOST_B),
      aSelectableAgent("claude", "Claude", HOST_C),
    ];

    // When
    const selected = agentForHost(agents, HOST_B, "claude");

    // Then — the name match is scoped to the host taking the session, not to the whole fleet
    expect(selected).toEqual(aSelectableAgent("reviewer", "Reviewer", HOST_B));
  });
});
