/**
 * The Create pane's agent metadata comes from the daemon's `ListAgents` answer, not from
 * the bundle: these tests pin the derivations over a served catalogue.
 */

import { describe, expect, it } from "bun:test";

import {
  agentForSessionType,
  bindsParam,
  hasCapability,
} from "./codingAgentMetadata";
import {
  cliSessionTypeOptions,
  sessionLaunchOf,
  sessionTypeOptions,
  type ServedSessionType,
} from "./cliHarnessOptions";
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

function aServedType(overrides: Partial<ServedSessionType>): ServedSessionType {
  return {
    sessionType: "acme-cli",
    label: "Acme",
    launch: "agent_cli",
    daemonInstanceId: "host-1",
    ...overrides,
  };
}

describe("sessionTypeOptions", () => {
  it("offers the session types the host declared, tddy-coder's first, each with its launch", () => {
    // Given a host declaring a coding agent's CLI and the tddy-coder workflow tool
    const served = [
      aServedType({}),
      aServedType({ sessionType: "flow", label: "Flow", launch: "tddy_coder" }),
    ];

    // When
    const options = sessionTypeOptions(served, "host-1", "");

    // Then
    expect(options).toEqual([
      { id: "flow", label: "Flow", launch: "tddy_coder" },
      { id: "acme-cli", label: "Acme", launch: "agent_cli" },
    ]);
  });

  it("offers nothing of another host's session types", () => {
    expect(sessionTypeOptions([aServedType({})], "host-2", "")).toEqual([]);
  });

  it("names the coding agents' CLI types among them by their launch, not their spelling", () => {
    const options = sessionTypeOptions(
      [aServedType({ sessionType: "acme" }), aServedType({ sessionType: "x-cli", launch: "tddy_coder" })],
      "host-1",
      "",
    );
    expect(cliSessionTypeOptions(options).map((option) => option.id)).toEqual(["acme"]);
  });
});

describe("sessionLaunchOf", () => {
  it("reads the served launch, the no-agent word, and nothing for an undeclared type", () => {
    const options = sessionTypeOptions(
      [aServedType({}), aServedType({ sessionType: "flow", launch: "tddy_coder" })],
      "host-1",
      "",
    );
    expect([
      sessionLaunchOf("acme-cli", options),
      sessionLaunchOf("flow", options),
      sessionLaunchOf("workspace", options),
      sessionLaunchOf("other-cli", options),
      sessionLaunchOf(undefined, options),
    ]).toEqual(["agent_cli", "tddy_coder", "no_agent", undefined, undefined]);
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
