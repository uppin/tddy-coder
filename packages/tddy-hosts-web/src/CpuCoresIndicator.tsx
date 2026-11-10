/**
 * CpuCoresIndicator — presentational strip of one mini bar per logical core for the Host Stats
 * Footer. Each bar's height encodes its core's utilization percentage.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import React from "react";
import { clampCorePercent } from "./hostStatsFormat";

export interface CpuCoresIndicatorProps {
  /** Utilization percentage (0..100) of each logical core, core 0 first. */
  perCorePercent: number[];
}

export function CpuCoresIndicator({ perCorePercent }: CpuCoresIndicatorProps) {
  return (
    <div data-testid="cpu-cores" className="flex items-end gap-0.5 h-4" title="Per-core CPU usage">
      {perCorePercent.map((raw, index) => {
        const percent = clampCorePercent(raw);
        return (
          <div
            key={index}
            data-testid={`cpu-core-bar-${index}`}
            data-percent={String(percent)}
            className="w-1 rounded-sm bg-muted-foreground/70"
            style={{ height: `${percent}%` }}
          />
        );
      })}
    </div>
  );
}
