/**
 * SessionUsageTab — the Session Inspector "Usage" tab: a live per-conversation token-usage
 * breakdown with a summing TOTAL row. Driven by the latest `StreamSessionUsage` snapshot; renders a
 * zero state until the first snapshot arrives.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import React from "react";
import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "tddy-client-web/gen/activity_pb";
import { Table, TableBody, TableCell, TableFooter, TableHead, TableHeader, TableRow, InlineError } from "tddy-ui-web";
import { formatTokens } from "./formatTokens";
import { buildUsageTree, usageTotals } from "./sessionUsage";
import { useSessionUsage } from "./useSessionUsage";

export interface SessionUsageTabProps {
  /** `activity.ActivityService` on the host that owns the session. */
  client: Client<typeof ActivityService> | undefined;
  sessionToken: string;
  sessionId: string;
}

/**
 * Owns the usage subscription itself (rather than receiving a snapshot prop) so the
 * usage stream opens only while this tab is mounted — i.e. when the user is actually
 * viewing Usage — instead of for the whole lifetime of the inspector drawer.
 */
export function SessionUsageTab({ client, sessionToken, sessionId }: SessionUsageTabProps) {
  const { usage, error } = useSessionUsage(client, sessionToken, sessionId);
  // Quiet, beside the table: the last snapshot stays readable while the stream is down.
  const streamError = (
    <InlineError
      testId="sessions-usage-stream-error"
      message={error && `Usage unavailable: ${error}`}
      className="text-xs"
    />
  );

  if (usage.length === 0) {
    return (
      <div
        data-testid="sessions-usage-tab-panel"
        className="px-3 py-3 flex flex-col gap-4"
      >
        {/* A stream that failed says so, rather than claiming the session reported nothing. */}
        {error ? (
          streamError
        ) : (
          <div
            data-testid="sessions-usage-empty"
            className="text-xs text-muted-foreground"
          >
            No token usage reported yet.
          </div>
        )}
      </div>
    );
  }

  const totals = usageTotals(usage);
  const rows = buildUsageTree(usage);
  const head = "text-xs text-muted-foreground font-medium text-right";
  const num = "text-right tabular-nums";
  const cost = (usd: number | undefined) => (usd === undefined ? "" : `$${usd.toFixed(2)}`);

  return (
    <div
      data-testid="sessions-usage-tab-panel"
      className="px-3 py-3 flex flex-col gap-4"
    >
      {streamError}
      <Table className="text-xs">
        <TableHeader>
          <TableRow>
            <TableHead className="text-xs text-muted-foreground font-medium">Agent</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium">Model</TableHead>
            <TableHead className={head}>In</TableHead>
            <TableHead className={head}>Cache read</TableHead>
            <TableHead className={head}>Cache write</TableHead>
            <TableHead className={head}>Out</TableHead>
            <TableHead className={head}>Reasoning</TableHead>
            <TableHead className={head}>Total</TableHead>
            <TableHead className={head}>Cost</TableHead>
            <TableHead className={head}>Turns</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((c) => (
            <TableRow key={c.id} data-testid={`sessions-usage-row-${c.id}`} data-depth={c.depth}>
              <TableCell
                data-testid={`sessions-usage-row-agent-${c.id}`}
                style={{ paddingLeft: `${c.depth * 12 + 8}px` }}
              >
                {c.agent}
              </TableCell>
              <TableCell
                data-testid={`sessions-usage-row-model-${c.id}`}
                className="text-muted-foreground"
              >
                {c.model}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-input-${c.id}`} className={num}>
                {formatTokens(c.inputTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-cache-read-${c.id}`} className={num}>
                {formatTokens(c.cacheReadTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-cache-write-${c.id}`} className={num}>
                {formatTokens(c.cacheWriteTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-output-${c.id}`} className={num}>
                {formatTokens(c.outputTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-reasoning-${c.id}`} className={num}>
                {formatTokens(c.reasoningTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-total-${c.id}`} className={num}>
                {formatTokens(c.totalTokens)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-cost-${c.id}`} className={num}>
                {cost(c.costUsd)}
              </TableCell>
              <TableCell data-testid={`sessions-usage-row-turns-${c.id}`} className={num}>
                {String(c.turns)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell className="font-medium">TOTAL</TableCell>
            <TableCell />
            <TableCell data-testid="sessions-usage-total-input" className={num}>
              {formatTokens(totals.inputTokens)}
            </TableCell>
            <TableCell data-testid="sessions-usage-total-cache-read" className={num}>
              {formatTokens(totals.cacheReadTokens)}
            </TableCell>
            <TableCell data-testid="sessions-usage-total-cache-write" className={num}>
              {formatTokens(totals.cacheWriteTokens)}
            </TableCell>
            <TableCell data-testid="sessions-usage-total-output" className={num}>
              {formatTokens(totals.outputTokens)}
            </TableCell>
            <TableCell data-testid="sessions-usage-total-reasoning" className={num}>
              {formatTokens(totals.reasoningTokens)}
            </TableCell>
            <TableCell data-testid="sessions-usage-total-total" className={num}>
              {formatTokens(totals.totalTokens)}
            </TableCell>
            <TableCell
              data-testid="sessions-usage-total-cost"
              className={num}
              title={
                totals.costPartial
                  ? "partial: some conversations did not report a cost"
                  : undefined
              }
            >
              {totals.costUsd === null
                ? ""
                : `${cost(totals.costUsd)}${totals.costPartial ? "*" : ""}`}
            </TableCell>
            <TableCell />
          </TableRow>
        </TableFooter>
      </Table>
    </div>
  );
}
