/**
 * HostStatsFooter — the screen-level bottom strip on `SessionsDrawerScreen`. Hosts the host-level
 * indicators for the currently selected daemon: available disk space, memory, load average and
 * per-core CPU utilization. What sessions put on the same strip (the byte-traffic readout, the
 * upload progress) arrives through `leading` / `trailing`, so this package knows nothing of sessions.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import React from "react";
import { DiskSpaceIndicator } from "./DiskSpaceIndicator";
import { MemoryIndicator } from "./MemoryIndicator";
import { LoadAverageIndicator } from "./LoadAverageIndicator";
import { CpuCoresIndicator } from "./CpuCoresIndicator";
import { useHostStats } from "./useHostStats";

export interface HostStatsFooterProps {
  /** Rendered before the indicators — the session traffic readout. */
  leading?: React.ReactNode;
  /** Rendered after the indicators — the aggregate upload progress. */
  trailing?: React.ReactNode;
}

export function HostStatsFooter({ leading = null, trailing = null }: HostStatsFooterProps) {
  const { perCorePercent, disk, memory, load } = useHostStats();

  return (
    <div
      data-testid="host-stats-footer"
      className="flex-shrink-0 flex items-center gap-3 px-2 py-1 border-t border-border"
    >
      {leading}
      <DiskSpaceIndicator availableBytes={disk ? disk.availableBytes : null} />
      <MemoryIndicator memory={memory} />
      <LoadAverageIndicator load={load} />
      <CpuCoresIndicator perCorePercent={perCorePercent} />
      {trailing}
    </div>
  );
}
