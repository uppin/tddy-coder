/**
 * Available memory for a host, beside the disk and CPU indicators.
 *
 * Same shape as `DiskSpaceIndicator` deliberately — both answer "how much headroom is left" and
 * share `formatBytesFree`, so the two readouts cannot drift into different phrasings of the same
 * idea.
 */

import React from "react";
import type { HostMemoryStats } from "../../rpc/useHostStats";
import { formatBytesFree } from "./hostStatsFormat";

export interface MemoryIndicatorProps {
  /** Latest memory reading, or `null` when the host has not reported one. */
  memory: HostMemoryStats | null;
}

export function MemoryIndicator({ memory }: MemoryIndicatorProps) {
  return (
    <span data-testid="memory-indicator" className="text-xs text-muted-foreground">
      {memory === null ? "—" : formatBytesFree(memory.availableBytes)}
    </span>
  );
}
