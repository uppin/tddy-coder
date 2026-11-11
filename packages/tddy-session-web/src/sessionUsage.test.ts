/**
 * Unit tests for the pure token-usage model the Usage tab renders from: the tree its rows are
 * shown as and the TOTAL row.
 *
 * Changeset: `usage-binding-daemon-stream` (`#usage-binding 2/5`)
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import { describe, it, expect } from "bun:test";

import {
  buildUsageTree,
  usageTotals,
  emptyUsage,
  type ConversationUsage,
} from "./sessionUsage";

/** A conversation with `id`, every counter zero unless overridden. */
function aConversation(id: string, overrides: Partial<ConversationUsage> = {}): ConversationUsage {
  return {
    agent: "agent",
    id,
    model: "m",
    inputTokens: 0n,
    cacheReadTokens: 0n,
    cacheWriteTokens: 0n,
    outputTokens: 0n,
    reasoningTokens: 0n,
    totalTokens: 0n,
    turns: 0,
    ...overrides,
  };
}

/** Each row as `id@depth`, in order. */
function shape(rows: { id: string; depth: number }[]): string[] {
  return rows.map((r) => `${r.id}@${r.depth}`);
}

describe("buildUsageTree", () => {
  it("puts each child after its parent with its depth", () => {
    // Given — a grandchild listed before its parent, and a second top-level conversation
    const records = [
      aConversation("main"),
      aConversation("grandchild", { parentId: "child" }),
      aConversation("child", { parentId: "main" }),
      aConversation("fc-1"),
    ];

    // When
    const rows = buildUsageTree(records);

    // Then
    expect(shape(rows)).toEqual(["main@0", "child@1", "grandchild@2", "fc-1@0"]);
  });

  it("makes an orphan child a top-level row", () => {
    // Given
    const records = [aConversation("main"), aConversation("lost", { parentId: "gone" })];

    // When
    const rows = buildUsageTree(records);

    // Then
    expect(shape(rows)).toEqual(["main@0", "lost@0"]);
  });
});

describe("usageTotals", () => {
  it("sums every counter across the conversations", () => {
    // Given
    const records = [
      aConversation("main", {
        inputTokens: 12n,
        cacheReadTokens: 300n,
        cacheWriteTokens: 40n,
        outputTokens: 100n,
        totalTokens: 452n,
      }),
      aConversation("thr", { inputTokens: 8n, outputTokens: 50n, reasoningTokens: 20n, totalTokens: 78n }),
    ];

    // When
    const totals = usageTotals(records);

    // Then
    expect(totals).toEqual({
      inputTokens: 20n,
      cacheReadTokens: 300n,
      cacheWriteTokens: 40n,
      outputTokens: 150n,
      reasoningTokens: 20n,
      totalTokens: 530n,
      costUsd: null,
      costPartial: false,
    });
  });

  it("flags a partial cost when only some conversations reported one", () => {
    // Given
    const records = [
      aConversation("a", { costUsd: 0.25 }),
      aConversation("b", { costUsd: 0.5 }),
      aConversation("c"),
    ];

    // When
    const { costUsd, costPartial } = usageTotals(records);

    // Then
    expect({ costUsd, costPartial }).toEqual({ costUsd: 0.75, costPartial: true });
  });

  it("is all zeros with no cost for an empty snapshot", () => {
    expect(usageTotals(emptyUsage())).toEqual({
      inputTokens: 0n,
      cacheReadTokens: 0n,
      cacheWriteTokens: 0n,
      outputTokens: 0n,
      reasoningTokens: 0n,
      totalTokens: 0n,
      costUsd: null,
      costPartial: false,
    });
  });
});
