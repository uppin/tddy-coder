/**
 * Owns the access + refresh token pair and hands out a valid access token on demand.
 *
 * The access token is short-lived (5 min) and sent on every RPC; the refresh token is long-lived
 * (7-day sliding) and used only to mint fresh access tokens. `ensureFreshAccessToken()` returns the
 * stored access token while it is still fresh, and otherwise performs a single-flight
 * `RefreshSession` — swapping in the new access + slid refresh token — so a call made right after
 * the device wakes transparently refreshes instead of failing. When the refresh token itself is
 * rejected, both tokens are cleared and the caller is told the session has ended.
 *
 * Storage, the auth client, and the clock are injected so the store is unit-testable without a DOM.
 */

import type { Client } from "@connectrpc/connect";
import { AuthService } from "../gen/auth_pb";

/** Persistence seam for the token pair (production backs this with `localStorage`). */
export interface TokenStorage {
  getAccess(): string | null;
  getRefresh(): string | null;
  set(access: string, refresh: string): void;
  clear(): void;
}

export interface SessionTokenStoreDeps {
  authClient: Client<typeof AuthService>;
  storage: TokenStorage;
  /** Current time in epoch milliseconds. Defaults to `Date.now`. */
  now?: () => number;
  /** Called once when a refresh fails because the refresh token is expired/invalid. */
  onLoggedOut?: () => void;
  /** Notified when a refresh starts (`true`) and ends (`false`) — drives the refreshing indicator. */
  onRefreshingChange?: (refreshing: boolean) => void;
  /** Notified with the new access token whenever a refresh installs one — keeps consumers in sync. */
  onAccessTokenChange?: (accessToken: string) => void;
}

export interface SessionTokenStore {
  /**
   * Resolve the access token to send on a request: the stored one while it is still fresh, a newly
   * minted one (single-flight `RefreshSession`) when it has lapsed and a refresh token exists, or
   * the stored access token as-is when there is no refresh token to mint from (nothing better is
   * possible — the server decides). `null` only when neither token is present.
   */
  ensureFreshAccessToken(): Promise<string | null>;
  /** True while a `RefreshSession` is in flight. */
  isRefreshing(): boolean;
}

/**
 * Refresh once the access token is within this many milliseconds of its `exp` (or already past it),
 * so a request never leaves with a token about to be rejected server-side.
 */
const EXPIRY_SKEW_MS = 30 * 1000;

/** Decode a token's `exp` (Unix seconds) from its payload segment, without verifying the signature. */
function decodeExpMs(token: string): number | null {
  const parts = token.split(".");
  if (parts.length !== 3) {
    return null;
  }
  const payload = base64UrlDecode(parts[1]);
  if (payload === null) {
    return null;
  }
  const exp = (JSON.parse(payload) as { exp?: unknown }).exp;
  return typeof exp === "number" ? exp * 1000 : null;
}

function base64UrlDecode(segment: string): string | null {
  try {
    const padded = segment + "=".repeat((4 - (segment.length % 4)) % 4);
    return atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
  } catch {
    return null;
  }
}

export function createSessionTokenStore(deps: SessionTokenStoreDeps): SessionTokenStore {
  const { authClient, storage, onLoggedOut, onRefreshingChange, onAccessTokenChange } = deps;
  const now = deps.now ?? Date.now;

  // Holds the shared promise while a refresh is in flight so concurrent callers await one call.
  let inFlight: Promise<string> | null = null;

  function accessTokenIsFresh(token: string): boolean {
    const expMs = decodeExpMs(token);
    // An undecodable token can't be trusted as fresh — force a refresh.
    return expMs !== null && expMs - now() > EXPIRY_SKEW_MS;
  }

  async function refresh(): Promise<string> {
    const refreshToken = storage.getRefresh() ?? "";
    onRefreshingChange?.(true);
    try {
      const res = await authClient.refreshSession({ refreshToken });
      storage.set(res.sessionToken, res.refreshToken);
      onAccessTokenChange?.(res.sessionToken);
      return res.sessionToken;
    } catch (err) {
      storage.clear();
      onLoggedOut?.();
      throw err;
    } finally {
      onRefreshingChange?.(false);
    }
  }

  return {
    isRefreshing() {
      return inFlight !== null;
    },
    ensureFreshAccessToken() {
      const access = storage.getAccess();
      if (access && accessTokenIsFresh(access)) {
        return Promise.resolve<string | null>(access);
      }
      // Without a refresh token there is nothing to mint from — send the current access token as-is
      // (may be null) and let the server reject it if it is truly invalid.
      if (!storage.getRefresh()) {
        return Promise.resolve<string | null>(access);
      }
      if (!inFlight) {
        inFlight = refresh().finally(() => {
          inFlight = null;
        });
      }
      return inFlight;
    },
  };
}
