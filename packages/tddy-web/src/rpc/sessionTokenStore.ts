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
 * Beside the pair it keeps the **vault unlock key** the daemon handed this session lineage: a wrap
 * key for the lineage's slot in the daemon's credential vault — not a stored credential, and
 * useless without the vault file on the daemon's disk. Every refresh presents it, so a daemon that
 * restarted can reopen the operator's stored credentials without a new login, and every refresh
 * replaces it with the rotated one the daemon returns. A lineage that has none — its vault was
 * locked at sign-in — gets one by unlocking the vault with its passphrase (`unlockVault`).
 *
 * It also reports where the vault stands (`onVaultStateChange`), from every refresh, unlock and
 * reset, so the page can ask for the passphrase while the vault is locked or not created yet.
 *
 * Storage, the auth client, and the clock are injected so the store is unit-testable without a DOM.
 */

import { Code, ConnectError, type Client } from "@connectrpc/connect";
import { AuthService, VaultState } from "../gen/auth_pb";

/** Persistence seam for the token pair (production backs this with `localStorage`). */
export interface TokenStorage {
  getAccess(): string | null;
  getRefresh(): string | null;
  /** The vault unlock key the last login or refresh returned; `null` when none is held. */
  getVaultUnlockKey(): string | null;
  /** Replace all three together — an empty `vaultUnlockKey` means the daemon handed none back. */
  set(access: string, refresh: string, vaultUnlockKey: string): void;
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
  /** Notified with where the credential vault stands, whenever a refresh, unlock or reset says. */
  onVaultStateChange?: (vaultState: VaultState) => void;
}

export interface SessionTokenStore {
  /**
   * Resolve the access token to send on a request: the stored one while it is still fresh, a newly
   * minted one (single-flight `RefreshSession`) when it has lapsed and a refresh token exists, or
   * the stored access token as-is when there is no refresh token to mint from (nothing better is
   * possible — the server decides). `null` only when neither token is present.
   */
  ensureFreshAccessToken(): Promise<string | null>;
  /**
   * Refresh now, whether or not the access token is still fresh (single-flight like the above).
   * A page load does this when it holds a vault unlock key, so a daemon that restarted since the
   * last refresh reopens the operator's credentials straight away rather than when the access
   * token next lapses.
   */
  refreshNow(): Promise<string>;
  /**
   * A page load's refresh: only when a vault unlock key is held, so a daemon that restarted since
   * the last refresh reopens the operator's credentials now rather than when the access token next
   * lapses. A failure is not the page load's to report — a definitive one already ended the session
   * through `onLoggedOut`, and a transient one is retried at the next refresh.
   */
  reopenVaultOnLoad(): Promise<void>;
  /**
   * Open the vault with its passphrase — or, with `create`, create it under a first one — and keep
   * the unlock key the daemon hands this lineage. A refusal (a wrong passphrase) rejects, with
   * nothing stored.
   */
  unlockVault(passphrase: string, create: boolean): Promise<void>;
  /** Set the vault aside on the daemon and open a fresh one under `newPassphrase`; keeps its key. */
  resetVault(newPassphrase: string): Promise<void>;
  /**
   * Sign out: hand the daemon this lineage's unlock key so it removes the slot — even when the
   * access token has lapsed, since the key identifies the slot itself — then clear every token,
   * whether or not the daemon answered.
   */
  logout(): Promise<void>;
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
  const { authClient, storage, onLoggedOut, onRefreshingChange, onAccessTokenChange, onVaultStateChange } = deps;
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
    const vaultUnlockKey = storage.getVaultUnlockKey() ?? "";
    onRefreshingChange?.(true);
    try {
      const res = await authClient.refreshSession({ refreshToken, vaultUnlockKey });
      // The presented unlock key opens nothing once the daemon has rotated it, so the returned one
      // always replaces it — including an empty one, when the daemon could not reopen the vault.
      storage.set(res.sessionToken, res.refreshToken, res.vaultUnlockKey);
      onAccessTokenChange?.(res.sessionToken);
      onVaultStateChange?.(res.vaultState);
      return res.sessionToken;
    } catch (err) {
      // Only a definitive server rejection of the refresh token (Unauthenticated) means the
      // session is truly over — clear both tokens and report logout. A transient failure (offline,
      // server briefly unreachable, a mobile tab waking on a flaky connection) must NOT discard the
      // still-valid 7-day refresh token: rethrow intact so the caller can retry later. Wiping tokens
      // on a momentary blip is exactly what forces a spurious re-login.
      if (ConnectError.from(err).code === Code.Unauthenticated) {
        storage.clear();
        onLoggedOut?.();
      }
      throw err;
    } finally {
      onRefreshingChange?.(false);
    }
  }

  function refreshSingleFlight(): Promise<string> {
    if (!inFlight) {
      inFlight = refresh().finally(() => {
        inFlight = null;
      });
    }
    return inFlight;
  }

  function ensureFreshAccessToken(): Promise<string | null> {
    const access = storage.getAccess();
    if (access && accessTokenIsFresh(access)) {
      return Promise.resolve<string | null>(access);
    }
    // Without a refresh token there is nothing to mint from — send the current access token as-is
    // (may be null) and let the server reject it if it is truly invalid.
    if (!storage.getRefresh()) {
      return Promise.resolve<string | null>(access);
    }
    return refreshSingleFlight();
  }

  /** Keep the unlock key an unlock or a reset returned, beside the tokens it belongs with. */
  function adoptUnlockKey(vaultUnlockKey: string, vaultState: VaultState) {
    storage.set(storage.getAccess() ?? "", storage.getRefresh() ?? "", vaultUnlockKey);
    onVaultStateChange?.(vaultState);
  }

  return {
    isRefreshing() {
      return inFlight !== null;
    },
    refreshNow() {
      return refreshSingleFlight();
    },
    async reopenVaultOnLoad() {
      if (!storage.getVaultUnlockKey() || !storage.getRefresh()) return;
      await refreshSingleFlight().catch(() => undefined);
    },
    async unlockVault(passphrase: string, create: boolean) {
      const sessionToken = (await ensureFreshAccessToken()) ?? "";
      const res = await authClient.unlockVault({ sessionToken, passphrase, create });
      adoptUnlockKey(res.vaultUnlockKey, res.vaultState);
    },
    async resetVault(newPassphrase: string) {
      const sessionToken = (await ensureFreshAccessToken()) ?? "";
      const res = await authClient.resetVault({ sessionToken, newPassphrase });
      adoptUnlockKey(res.vaultUnlockKey, res.vaultState);
    },
    async logout() {
      const sessionToken = storage.getAccess() ?? "";
      const vaultUnlockKey = storage.getVaultUnlockKey() ?? "";
      if (sessionToken || vaultUnlockKey) {
        try {
          await authClient.logout({ sessionToken, vaultUnlockKey });
        } catch {
          // The browser discards its tokens regardless; a slot left on the daemon opens nothing
          // without the key, and this is the last copy of the key.
        }
      }
      storage.clear();
    },
    ensureFreshAccessToken,
  };
}
