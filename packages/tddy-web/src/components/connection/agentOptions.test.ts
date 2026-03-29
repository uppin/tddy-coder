import { describe, expect, test } from "bun:test";
import {
  buildAgentSelectOptionsFromRpc,
  coalesceBackendAgentSelection,
} from "./agentOptions";

describe("buildAgentSelectOptionsFromRpc", () => {
  test("maps id and label from RPC agents", () => {
    const opts = buildAgentSelectOptionsFromRpc([
      { id: "a1", label: "Alpha" },
      { id: "b2", label: "Beta" },
    ]);
    expect(opts).toEqual([
      { value: "a1", label: "Alpha" },
      { value: "b2", label: "Beta" },
    ]);
  });
});

describe("coalesceBackendAgentSelection", () => {
  test("prefers previous id when still allowed", () => {
    const agents = [
      { value: "first", label: "First" },
      { value: "second", label: "Second" },
    ];
    expect(coalesceBackendAgentSelection(agents, "second")).toBe("second");
  });

  test("falls back to first agent when previous missing", () => {
    const agents = [{ value: "only", label: "Only" }];
    expect(coalesceBackendAgentSelection(agents, "stale")).toBe("only");
  });
});
