/** Parsed `codex_oauth` object from LiveKit participant metadata JSON. */
export interface CodexOAuthInfo {
  pending: boolean;
  authorizeUrl?: string;
  callbackPort?: number;
  state?: string;
}

/**
 * Parse participant `metadata` string for `codex_oauth` (see desktop / tddy-coder design).
 * Returns `null` if missing or invalid JSON.
 */
export function parseCodexOAuthMetadata(metadata: string): CodexOAuthInfo | null {
  const t = metadata.trim();
  if (!t) return null;
  try {
    const o = JSON.parse(t) as { codex_oauth?: Record<string, unknown> };
    const c = o.codex_oauth;
    if (!c || typeof c !== "object") return null;
    return {
      pending: Boolean(c.pending),
      authorizeUrl: typeof c.authorize_url === "string" ? c.authorize_url : undefined,
      callbackPort: typeof c.callback_port === "number" ? c.callback_port : undefined,
      state: typeof c.state === "string" ? c.state : undefined,
    };
  } catch {
    return null;
  }
}
