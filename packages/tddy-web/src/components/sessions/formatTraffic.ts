/**
 * Traffic display formatting helpers.
 *
 * Changeset: `byte-traffic`
 * PRD: `docs/ft/web/session-drawer.md` (Session Traffic Strip)
 */

/**
 * Format a byte count as a human-readable string.
 *
 * Thresholds:
 *   0            → "0 B"
 *   < 1000       → "N B"
 *   < 1_000_000  → "X.Y kB"
 *   < 1_000_000_000 → "X.Y MB"
 *   else         → "X.Y GB"
 *
 * Note: 999_999 formats as "1000.0 kB" (not MB) because it falls below 1_000_000.
 */
export function formatBytes(n: number): string {
  if (n === 0) return "0 B";
  if (n < 1_000) return `${Math.round(n)} B`;
  if (n < 1_000_000) return `${(n / 1_000).toFixed(1)} kB`;
  if (n < 1_000_000_000) return `${(n / 1_000_000).toFixed(1)} MB`;
  return `${(n / 1_000_000_000).toFixed(1)} GB`;
}

/**
 * Format a byte-per-second rate as a human-readable string.
 * Same thresholds and rounding as `formatBytes`, with a `/s` suffix.
 */
export function formatRate(n: number): string {
  if (n === 0) return "0 B/s";
  if (n < 1_000) return `${Math.round(n)} B/s`;
  if (n < 1_000_000) return `${(n / 1_000).toFixed(1)} kB/s`;
  if (n < 1_000_000_000) return `${(n / 1_000_000).toFixed(1)} MB/s`;
  return `${(n / 1_000_000_000).toFixed(1)} GB/s`;
}

/**
 * Format a round-trip time in milliseconds.
 *
 *   null → "—" (em dash placeholder)
 *   number → "N ms" (integer, floored)
 */
export function formatPing(ms: number | null): string {
  if (ms === null) return "—";
  return `${Math.floor(ms)} ms`;
}
