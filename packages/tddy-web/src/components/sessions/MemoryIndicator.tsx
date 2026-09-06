/**
 * Available memory for a host, beside the disk and CPU indicators.
 *
 * Same shape as `DiskSpaceIndicator` deliberately — both answer "how much headroom is left" and
 * share `formatBytesFree`, so the two readouts cannot drift into different phrasings of the same
 * idea.
 */

import type { HostMemoryStats } from "../../rpc/useHostStats";

export interface MemoryIndicatorProps {
  /** Latest memory reading, or `null` when the host has not reported one. */
  memory: HostMemoryStats | null;
}

export function MemoryIndicator({ memory }: MemoryIndicatorProps) {
  // TODO(host-resources): implement
  void memory;
  return <span data-testid="memory-indicator" />;
}
