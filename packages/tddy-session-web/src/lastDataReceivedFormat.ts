/**
 * Relative-time formatter for the inspector's "last data received: Ns ago" line.
 *
 * Changeset: `2026-07-12-fast-session-change`
 */

/**
 * Format `lastDataReceivedAt` (epoch-ms) as a relative "ago" phrase relative to `now` (epoch-ms).
 *
 * - `null` → `"never"` (no data has ever been received).
 * - future timestamp (clock skew) → `"just now"` (clamped, never negative).
 * - `< 60s` → `"Ns ago"`.
 * - `< 1h` → `"Nm ago"`.
 * - `< 1d` → `"Nh ago"`.
 * - `>= 1d` → `"Nd ago"`.
 */
export function formatLastDataReceived(
  lastDataReceivedAt: number | null,
  now: number,
): string {
  if (lastDataReceivedAt === null || lastDataReceivedAt === undefined) return "never";
  const deltaMs = now - lastDataReceivedAt;
  if (deltaMs < 0) return "just now";
  const secs = Math.floor(deltaMs / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
