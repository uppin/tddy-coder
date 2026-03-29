/**
 * Short session id label: for standard UUIDs (`8hex-4hex-…`), the first two hyphen-separated
 * groups lowercased; otherwise the substring before the first hyphen, or the whole id if there is
 * no hyphen.
 */
export function sessionIdFirstSegment(id: string): string {
  const t = id.trim();
  if (!t) return "—";
  const uuidTwo = t.match(/^([0-9a-fA-F]{8})-([0-9a-fA-F]{4})(?:-|$)/);
  if (uuidTwo) return `${uuidTwo[1].toLowerCase()}-${uuidTwo[2].toLowerCase()}`;
  const dash = t.indexOf("-");
  return dash === -1 ? t : t.slice(0, dash);
}

/** Formats API timestamps as `YYYY-MM-DD HH:MM:SS` in UTC (matches typical RFC3339 `Z` semantics). */
export function formatSessionCreatedAt(createdAt: string): string {
  const d = new Date(createdAt);
  if (Number.isNaN(d.getTime())) return createdAt;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

/** PID text only when the session is active and reports a non-zero process id; otherwise an em dash. */
export function sessionPidDisplay(isActive: boolean, pid: number): string {
  if (isActive && pid > 0) return String(pid);
  return "—";
}
