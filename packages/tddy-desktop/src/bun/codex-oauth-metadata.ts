/** Default loopback port when Codex prints `http://127.0.0.1:PORT/auth/callback`. */
export const DEFAULT_CODEX_OAUTH_CALLBACK_PORT = 1455;

/** Mirrors web `codexOauthMetadata` for the main process. */
export interface CodexOAuthInfo {
  pending: boolean;
  authorizeUrl?: string;
  callbackPort?: number;
  state?: string;
}

/** Port for the desktop callback server: ignore missing / zero / out-of-range metadata. */
export function resolvedCodexOAuthCallbackPort(
  info: Pick<CodexOAuthInfo, "callbackPort"> | null,
): number {
  const p = info?.callbackPort;
  return typeof p === "number" && p > 0 && p <= 65535
    ? p
    : DEFAULT_CODEX_OAUTH_CALLBACK_PORT;
}

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
