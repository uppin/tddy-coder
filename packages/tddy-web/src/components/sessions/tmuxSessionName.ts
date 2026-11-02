/**
 * The name "New tmux session…" prefills: `base` when no listed tmux session has it, else the first
 * of `base-2`, `base-3`, … that none has — so starting one never collides with a session already on
 * the host, and the name stays recognisable from a real terminal later.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-tmux-new-session-tab.md (`#session-tabs 7/7`)
 */
export function nextTmuxSessionName(base: string, existing: readonly string[]): string {
  // TODO(tmux-new): implement
  void existing;
  return base;
}
