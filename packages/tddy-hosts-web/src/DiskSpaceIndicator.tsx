/**
 * DiskSpaceIndicator — presentational free-disk-space readout for the Host Stats Footer.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import React from "react";
import { formatDiskFree } from "./hostStatsFormat";

export interface DiskSpaceIndicatorProps {
  /** Free bytes on the daemon's project-directory filesystem, or `null` before the first reading. */
  availableBytes: bigint | null;
}

export function DiskSpaceIndicator({ availableBytes }: DiskSpaceIndicatorProps) {
  return (
    <span data-testid="disk-space-available" className="text-xs text-muted-foreground">
      {availableBytes === null ? "—" : formatDiskFree(availableBytes)}
    </span>
  );
}
