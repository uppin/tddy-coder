/**
 * Unit tests for the pure token-usage reducer the Usage tab renders from.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import { describe, it, expect } from "bun:test";

import { usageTotals, emptyUsage, type ConversationUsage } from "./sessionUsage";

const CLAUDE: ConversationUsage = {
  agent: "claude",
  id: "claude-main",
  model: "claude-opus-4-8",
  inputTokens: 12_340n,
  outputTokens: 3_210n,
  totalTokens: 15_550n,
  turns: 7,
};

const EXPLORE: ConversationUsage = {
  agent: "Explore",
  id: "agent-01",
  model: "claude-haiku-4-5",
  inputTokens: 4_100n,
  outputTokens: 820n,
  totalTokens: 4_920n,
  turns: 2,
};

describe("usageTotals", () => {
  it("sums input, output and total tokens across every conversation", () => {
    expect(usageTotals([CLAUDE, EXPLORE])).toEqual({
      inputTokens: 16_440n,
      outputTokens: 4_030n,
      totalTokens: 20_470n,
    });
  });

  it("is all zeros for an empty snapshot", () => {
    expect(usageTotals(emptyUsage())).toEqual({
      inputTokens: 0n,
      outputTokens: 0n,
      totalTokens: 0n,
    });
  });
});
