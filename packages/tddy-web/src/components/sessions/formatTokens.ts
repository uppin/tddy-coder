/**
 * Token-count display formatting for the Session Inspector "Usage" tab.
 *
 * Changeset: `session-usage-inspector`
 * PRD: `docs/ft/web/session-usage-inspector.md`
 */

/** Render a token count as an en-US thousands-grouped string (e.g. `12340` -> `"12,340"`). */
export function formatTokens(n: number | bigint): string {
  return n.toLocaleString("en-US");
}
