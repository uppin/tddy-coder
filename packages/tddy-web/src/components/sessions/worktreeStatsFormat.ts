// Human-readable formatting for worktree stats (shared by the Worktrees manager screen and the
// session inspector's Worktree tab). See docs/ft/web/session-worktree-inspector.md.

/** Format a byte count as a compact size label (e.g. `1.2 GB`). Returns `—` for invalid input. */
export function formatDiskBytes(n: bigint): string {
  const v = Number(n);
  if (!Number.isFinite(v) || v < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let x = v;
  let i = 0;
  while (x >= 1024 && i < units.length - 1) {
    x /= 1024;
    i += 1;
  }
  const rounded = i === 0 ? Math.round(x) : Math.round(x * 10) / 10;
  return `${rounded} ${units[i]}`;
}
