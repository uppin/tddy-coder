/**
 * Pure token-usage model the Usage tab renders from.
 *
 * The daemon streams the full cumulative per-conversation snapshot over
 * `ActivityService.StreamSessionUsage`; this module holds the client-side shape, the tree the rows
 * are shown as, and the derived totals row.
 *
 * Changeset: `usage-binding-daemon-stream` (`#usage-binding 2/5`)
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

/**
 * One conversation's token accounting — the main agent or a subagent, at any depth. The counters
 * are disjoint (input is fresh input only; output excludes reasoning), so `totalTokens` is their
 * sum.
 */
export interface ConversationUsage {
  agent: string;
  id: string;
  model: string;
  inputTokens: bigint;
  cacheReadTokens: bigint;
  cacheWriteTokens: bigint;
  outputTokens: bigint;
  reasoningTokens: bigint;
  totalTokens: bigint;
  turns: number;
  /** US dollars, when the source reported a cost. */
  costUsd?: number;
  /** The conversation this one is nested under. */
  parentId?: string;
}

/** A conversation placed in the tree: `depth` 0 is a top-level conversation. */
export interface UsageRow extends ConversationUsage {
  depth: number;
}

/** The TOTAL row: every counter summed, and the cost of the rows that reported one. */
export interface UsageTotals {
  inputTokens: bigint;
  cacheReadTokens: bigint;
  cacheWriteTokens: bigint;
  outputTokens: bigint;
  reasoningTokens: bigint;
  totalTokens: bigint;
  /** The summed cost, or `null` when no row reported one. */
  costUsd: number | null;
  /** Some rows reported a cost and some did not — the summed cost understates the session's. */
  costPartial: boolean;
}

/** The initial (no-usage-yet) snapshot. */
export function emptyUsage(): ConversationUsage[] {
  return [];
}

/**
 * The snapshot as the rows the tab shows: each conversation followed by its children, recursively,
 * siblings in snapshot order. A child whose parent is not in the snapshot is a top-level row.
 */
export function buildUsageTree(records: ConversationUsage[]): UsageRow[] {
  // TODO(daemon-stream): implement
  void records;
  return [];
}

/** Sum every counter across the snapshot, and the cost of the rows that reported one. */
export function usageTotals(records: ConversationUsage[]): UsageTotals {
  // TODO(daemon-stream): implement
  void records;
  return {
    inputTokens: 0n,
    cacheReadTokens: 0n,
    cacheWriteTokens: 0n,
    outputTokens: 0n,
    reasoningTokens: 0n,
    totalTokens: 0n,
    costUsd: null,
    costPartial: false,
  };
}
