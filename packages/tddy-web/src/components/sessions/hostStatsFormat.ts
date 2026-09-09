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
  return formatBytesFree(availableBytes);
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

/**
 * A byte count as a short "free" phrase, for any host resource.
 *
 * Shares `formatDiskFree`'s signature (`number | bigint`) so a `uint64` from the wire needs no
 * `Number()` conversion at the call site, and so memory and disk cannot drift into two different
 * renderings of the same quantity.
 */
export function formatBytesFree(availableBytes: number | bigint): string {
  const bytes = typeof availableBytes === "bigint" ? Number(availableBytes) : availableBytes;
  return `${formatBytes(bytes)} free`;
}

/**
 * A load average as it should read in a row, or `null` when the host reports none.
 *
 * Returning `null` rather than `"0.00"` is the whole point: a host with no load average must not be
 * indistinguishable from an idle one.
 */
export function formatLoadAverage(load: { oneMinute: number } | null): string | null {
  if (load === null) return null;
  return load.oneMinute.toFixed(2);
}
