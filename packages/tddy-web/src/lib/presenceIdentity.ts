/**
 * LiveKit identity for web dashboard presence. Must be unique per browser tab
 * so a second tab does not evict the first (DUPLICATE_IDENTITY).
 */
export function presenceIdentityForUser(login: string): string {
  return `web-${login}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
