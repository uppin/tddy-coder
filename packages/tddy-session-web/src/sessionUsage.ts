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
  const ids = new Set(records.map((r) => r.id));
  const childrenOf = new Map<string, ConversationUsage[]>();
  const roots: ConversationUsage[] = [];
  for (const r of records) {
    if (r.parentId !== undefined && r.parentId !== "" && ids.has(r.parentId) && r.parentId !== r.id) {
      const siblings = childrenOf.get(r.parentId) ?? [];
      siblings.push(r);
      childrenOf.set(r.parentId, siblings);
    } else {
      roots.push(r);
    }
  }
  const rows: UsageRow[] = [];
  const visited = new Set<string>();
  const place = (r: ConversationUsage, depth: number) => {
    if (visited.has(r.id)) return;
    visited.add(r.id);
    rows.push({ ...r, depth });
    for (const child of childrenOf.get(r.id) ?? []) place(child, depth + 1);
  };
  for (const r of roots) place(r, 0);
  return rows;
}

/** Sum every counter across the snapshot, and the cost of the rows that reported one. */
export function usageTotals(records: ConversationUsage[]): UsageTotals {
  const totals: UsageTotals = {
    inputTokens: 0n,
    cacheReadTokens: 0n,
    cacheWriteTokens: 0n,
    outputTokens: 0n,
    reasoningTokens: 0n,
    totalTokens: 0n,
    costUsd: null,
    costPartial: false,
  };
  let withCost = 0;
  for (const r of records) {
    totals.inputTokens += r.inputTokens;
    totals.cacheReadTokens += r.cacheReadTokens;
    totals.cacheWriteTokens += r.cacheWriteTokens;
    totals.outputTokens += r.outputTokens;
    totals.reasoningTokens += r.reasoningTokens;
    totals.totalTokens += r.totalTokens;
    if (r.costUsd !== undefined) {
      totals.costUsd = (totals.costUsd ?? 0) + r.costUsd;
      withCost += 1;
    }
  }
  totals.costPartial = withCost > 0 && withCost < records.length;
  return totals;
}
