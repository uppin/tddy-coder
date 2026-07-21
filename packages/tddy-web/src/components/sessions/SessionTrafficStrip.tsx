/**
 * SessionTrafficStrip — a thin horizontal bar showing live traffic metrics
 * for an active LiveKit session: byte totals, byte rates, and ping.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

import React from "react";
import { formatBytes, formatRate, formatPing } from "./formatTraffic";

export interface SessionTrafficStripProps {
  bytesIn: number;
  bytesOut: number;
  inRate: number;
  outRate: number;
  pingMs: number | null;
}

export function SessionTrafficStrip({
  bytesIn,
  bytesOut,
  inRate,
  outRate,
  pingMs,
}: SessionTrafficStripProps) {
  return (
    <div
      data-testid="session-traffic-strip"
      className="flex items-center gap-3 px-2 py-1 flex-shrink-0 text-xs text-muted-foreground"
    >
      <span>
        <span data-testid="session-traffic-rate-out">{formatRate(outRate)}</span>
        {" ↑"}
      </span>
      <span>
        <span data-testid="session-traffic-rate-in">{formatRate(inRate)}</span>
        {" ↓"}
      </span>
      <span data-testid="session-traffic-bytes-out">{formatBytes(bytesOut)}</span>
      <span data-testid="session-traffic-bytes-in">{formatBytes(bytesIn)}</span>
      <span data-testid="session-traffic-ping">{formatPing(pingMs)}</span>
    </div>
  );
}
