/**
 * Formatting helpers for the Host Stats Footer's disk and per-core CPU readouts.
 *
 * PRD: `docs/ft/web/host-stats-footer.md`
 * Changeset: `host-stats-footer`
 */

import { formatBytes } from "./formatTraffic";

/**
 * Format available disk space as a human-readable free-space label, e.g. `"42.1 GB free"`.
 * Accepts a `bigint` (the proto `uint64`) or a plain `number`.
 */
export function formatDiskFree(availableBytes: number | bigint): string {
  const bytes = typeof availableBytes === "bigint" ? Number(availableBytes) : availableBytes;
  return `${formatBytes(bytes)} free`;
}

/**
 * Clamp a raw CPU utilization figure to the valid `[0, 100]` percentage range. Guards against a
 * provider reporting slightly out-of-range values.
 */
export function clampCorePercent(raw: number): number {
  if (raw < 0) return 0;
  if (raw > 100) return 100;
  return raw;
}
