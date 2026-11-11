/**
 * Unit tests for which session types can run a project's main, from the metadata the
 * daemon served for each agent.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import { describe, expect, it } from "bun:test";
import { whyMainCannotRun } from "./projectHomeSessionTypes";
import type { SelectableAgent } from "./selectableAgentOptions";

function anAgent(capabilities: string[]): SelectableAgent {
  return {
    id: "acme",
    label: "Acme",
    daemonInstanceId: "host-1",
    sessionType: "acme-cli",
    capabilities,
    runParams: [],
    permissionModes: [],
    defaultModel: "",
    placements: [],
  };
}

describe("whyMainCannotRun", () => {
  it("lets an agent whose spec declares project_home_main run main", () => {
    expect(
      whyMainCannotRun("agent_cli", anAgent(["project_home_main"])),
    ).toBeNull();
  });

  it("explains that an agent without the capability cannot run main", () => {
    expect(whyMainCannotRun("agent_cli", anAgent(["managed_workflow"]))).toBe(
      "This agent's spec does not offer it as a project's main: main needs an agent that declares it can stand as one.",
    );
  });

  it("explains that a tddy-coder session cannot be main", () => {
    expect(whyMainCannotRun("tddy_coder", undefined)).toBe(
      "A workflow session cannot be a project's main: main is a coding agent's own session on the project's repository.",
    );
  });

  it("explains that an unserved agent cannot run main", () => {
    expect(whyMainCannotRun("agent_cli", undefined)).toBe(
      "This agent's spec does not offer it as a project's main: main needs an agent that declares it can stand as one.",
    );
  });
});
