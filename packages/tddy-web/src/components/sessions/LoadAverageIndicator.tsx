/**
 * One-minute load average for a host, beside the memory, disk and CPU indicators.
 *
 * A host whose platform has no load average reports none, and that is rendered as `—`. It is never
 * rendered as `0.00`, which an operator reads as an idle machine — see CLAUDE.md on fallbacks.
 */

import React from "react";
import type { HostLoadStats } from "../../rpc/useHostStats";
import { formatLoadAverage } from "./hostStatsFormat";

export interface LoadAverageIndicatorProps {
  /** Latest load averages, or `null` when the host reports none. */
  load: HostLoadStats | null;
}

export function LoadAverageIndicator({ load }: LoadAverageIndicatorProps) {
  const formatted = formatLoadAverage(load);
  return (
    <span
      data-testid="load-average-indicator"
      className="text-xs text-muted-foreground"
      title="Load average (1 minute)"
    >
      {formatted === null ? "—" : `load ${formatted}`}
    </span>
  );
}
