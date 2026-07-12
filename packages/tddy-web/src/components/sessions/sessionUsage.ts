/**
 * Pure token-usage model the Usage tab renders from.
 *
 * The presenter streams the full cumulative per-conversation snapshot over `TddyRemote.Stream`
 * (a `tokenUsageUpdated` `ServerMessage`); this module holds the client-side shape and the
 * derived totals row.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

/** One conversation's token accounting — the main agent or a subagent. */
export interface ConversationUsage {
  agent: string;
  id: string;
  model: string;
  inputTokens: bigint;
  outputTokens: bigint;
  totalTokens: bigint;
  turns: number;
}

/** The initial (no-usage-yet) snapshot. */
export function emptyUsage(): ConversationUsage[] {
  return [];
}

/** Sum input, output and total tokens across every conversation in the snapshot. */
export function usageTotals(records: ConversationUsage[]): {
  inputTokens: bigint;
  outputTokens: bigint;
  totalTokens: bigint;
} {
  return records.reduce(
    (totals, record) => ({
      inputTokens: totals.inputTokens + record.inputTokens,
      outputTokens: totals.outputTokens + record.outputTokens,
      totalTokens: totals.totalTokens + record.totalTokens,
    }),
    { inputTokens: 0n, outputTokens: 0n, totalTokens: 0n },
  );
}
