/**
 * The Create pane's agent metadata comes from the daemon's `ListAgents` answer, not from
 * the bundle: these tests pin the derivations over a served catalogue.
 */

import { describe, expect, it } from "bun:test";

import {
  agentForSessionType,
  bindsParam,
  hasCapability,
  isCliSessionType,
} from "./codingAgentMetadata";
import { sessionTypeOptions } from "./cliHarnessOptions";
import type { SelectableAgent } from "./selectableAgentOptions";

function anAgent(overrides: Partial<SelectableAgent>): SelectableAgent {
  return {
    id: "acme",
    label: "Acme",
    daemonInstanceId: "host-1",
    sessionType: "acme-cli",
    capabilities: ["managed_workflow"],
    runParams: ["model"],
    permissionModes: [],
    defaultModel: "acme-1",
    placements: [],
    ...overrides,
  };
}

describe("sessionTypeOptions", () => {
  it("offers tool, then every PTY agent the host serves, labelled by its product", () => {
    // Given a host serving two agents, one with no PTY launch
    const agents = [
      anAgent({}),
      anAgent({ id: "acme-acp", sessionType: "" }),
    ];

    // When
    const options = sessionTypeOptions(agents, "host-1", "");

    // Then
    expect(options).toEqual([
      { id: "tool", label: "Tool" },
      { id: "acme-cli", label: "Acme" },
    ]);
  });

  it("offers nothing of another host's agents", () => {
    expect(sessionTypeOptions([anAgent({})], "host-2", "")).toEqual([
      { id: "tool", label: "Tool" },
    ]);
  });
});

describe("agentForSessionType", () => {
  it("answers the served agent behind a session type, and nothing for an unserved one", () => {
    const agents = [anAgent({})];
    expect(
      agentForSessionType(agents, "host-1", "", "acme-cli")?.id,
    ).toBe("acme");
    expect(
      agentForSessionType(agents, "host-1", "", "other-cli"),
    ).toBeUndefined();
  });
});

describe("isCliSessionType", () => {
  it("is the `<id>-cli` spelling, for any agent", () => {
    expect(isCliSessionType("acme-cli")).toBe(true);
    expect(isCliSessionType("acme")).toBe(false);
    expect(isCliSessionType("tool")).toBe(false);
    expect(isCliSessionType("")).toBe(false);
  });
});

describe("capability and param reads", () => {
  it("follow the served metadata", () => {
    const agent = anAgent({});
    expect(hasCapability(agent, "managed_workflow")).toBe(true);
    expect(hasCapability(agent, "split_host")).toBe(false);
    expect(bindsParam(agent, "model")).toBe(true);
    expect(bindsParam(agent, "permission_mode")).toBe(false);
    expect(hasCapability(undefined, "managed_workflow")).toBe(false);
  });
});
