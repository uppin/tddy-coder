/**
 * SessionUsageTab — the Session Inspector "Usage" tab: a live per-conversation token-usage
 * breakdown with a summing TOTAL row. Driven by the latest `tokenUsageUpdated` snapshot; renders a
 * zero state until the first snapshot arrives.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

import React from "react";
import type { Room } from "livekit-client";
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "../ui/table";
import { formatTokens } from "./formatTokens";
import { usageTotals } from "./sessionUsage";
import { useSessionUsage } from "./useSessionUsage";

export interface SessionUsageTabProps {
  /** LiveKit room + daemon/presenter identity selecting the usage stream's transport target. */
  room: Room | null;
  serverIdentity: string;
}

/**
 * Owns the usage subscription itself (rather than receiving a snapshot prop) so the
 * `TddyRemote.Stream` opens only while this tab is mounted — i.e. when the user is actually
 * viewing Usage — instead of for the whole lifetime of the inspector drawer.
 */
export function SessionUsageTab({ room, serverIdentity }: SessionUsageTabProps) {
  const usage = useSessionUsage(room, serverIdentity);


  if (usage.length === 0) {
    return (
      <div
        data-testid="sessions-usage-tab-panel"
        className="px-3 py-3 flex flex-col gap-4"
      >
        <div
          data-testid="sessions-usage-empty"
          className="text-xs text-muted-foreground"
        >
          No token usage reported yet.
        </div>
      </div>
    );
  }

  const totals = usageTotals(usage);

  return (
    <div
      data-testid="sessions-usage-tab-panel"
      className="px-3 py-3 flex flex-col gap-4"
    >
      <Table className="text-xs">
        <TableHeader>
          <TableRow>
            <TableHead className="text-xs text-muted-foreground font-medium">Agent</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium">Model</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium text-right">In</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium text-right">Out</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium text-right">Total</TableHead>
            <TableHead className="text-xs text-muted-foreground font-medium text-right">Turns</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {usage.map((c) => (
            <TableRow key={c.id} data-testid={`sessions-usage-row-${c.id}`}>
              <TableCell data-testid={`sessions-usage-row-agent-${c.id}`}>{c.agent}</TableCell>
              <TableCell
                data-testid={`sessions-usage-row-model-${c.id}`}
                className="text-muted-foreground"
              >
                {c.model}
              </TableCell>
              <TableCell
                data-testid={`sessions-usage-row-input-${c.id}`}
                className="text-right tabular-nums"
              >
                {formatTokens(c.inputTokens)}
              </TableCell>
              <TableCell
                data-testid={`sessions-usage-row-output-${c.id}`}
                className="text-right tabular-nums"
              >
                {formatTokens(c.outputTokens)}
              </TableCell>
              <TableCell
                data-testid={`sessions-usage-row-total-${c.id}`}
                className="text-right tabular-nums"
              >
                {formatTokens(c.totalTokens)}
              </TableCell>
              <TableCell
                data-testid={`sessions-usage-row-turns-${c.id}`}
                className="text-right tabular-nums"
              >
                {String(c.turns)}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell className="font-medium">TOTAL</TableCell>
            <TableCell />
            <TableCell
              data-testid="sessions-usage-total-input"
              className="text-right tabular-nums"
            >
              {formatTokens(totals.inputTokens)}
            </TableCell>
            <TableCell
              data-testid="sessions-usage-total-output"
              className="text-right tabular-nums"
            >
              {formatTokens(totals.outputTokens)}
            </TableCell>
            <TableCell
              data-testid="sessions-usage-total-total"
              className="text-right tabular-nums"
            >
              {formatTokens(totals.totalTokens)}
            </TableCell>
            <TableCell />
          </TableRow>
        </TableFooter>
      </Table>
    </div>
  );
}
