import { describe, expect, it } from "bun:test";
import {
  buildAgentSelectOptionsFromRpc,
  coalesceBackendAgentSelection,
} from "./agentOptions";

describe("buildAgentSelectOptionsFromRpc", () => {
  it("builds a select option for each RPC agent, using its id as the value and its label as the display text", () => {
    // Given
    const rpcAgents = [
      { id: "a1", label: "Alpha" },
      { id: "b2", label: "Beta" },
    ];

    // When
    const opts = buildAgentSelectOptionsFromRpc(rpcAgents);

    // Then
    expect(opts).toEqual([
      { value: "a1", label: "Alpha" },
      { value: "b2", label: "Beta" },
    ]);
  });
});

describe("coalesceBackendAgentSelection", () => {
  it("keeps the previously selected agent when it is still in the allowed list", () => {
    // Given
    const agents = [
      { value: "first", label: "First" },
      { value: "second", label: "Second" },
    ];

    // When
    const result = coalesceBackendAgentSelection(agents, "second");

    // Then
    expect(result).toBe("second");
  });

  it("falls back to the first available agent when the previous selection is no longer allowed", () => {
    // Given
    const agents = [{ value: "only", label: "Only" }];

    // When
    const result = coalesceBackendAgentSelection(agents, "stale");

    // Then
    expect(result).toBe("only");
  });
});
