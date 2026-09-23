/**
 * Unit tests for createSessionTokenStore — the client-side owner of the access + refresh token
 * pair. It hands out a valid access token on demand: returning the stored one while it is fresh,
 * and performing a single-flight `RefreshSession` (swapping in the new access + slid refresh
 * token) when it has expired. When the refresh token itself is rejected it clears both tokens and
 * reports the session ended.
 *
 * Storage, the auth client, and the clock are injected so the store is testable without a DOM.
 *
 * Changeset: `durable-web-session`
 * PRD: `docs/ft/daemon/1-WIP/PRD-2026-07-04-durable-web-session.md`
 */

import { describe, it, expect } from "bun:test";
import { createClient, ConnectError, Code } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { AuthService, VaultState } from "../gen/auth_pb";

import { createSessionTokenStore, type TokenStorage } from "./sessionTokenStore";

// ---------------------------------------------------------------------------
// Token fixtures — real `v1.<base64url(payload)>.<sig>` strings the store decodes for `exp`/`kind`.
// ---------------------------------------------------------------------------

const FAR_FUTURE_EXP = 4102444800; // year 2100 (seconds)
const PAST_EXP = 1000;
const NOW_MS = 1_600_000_000 * 1000; // year 2020 — after PAST_EXP, before FAR_FUTURE_EXP

function mintToken(kind: "access" | "refresh", exp: number, jti: string): string {
  const claims = { id: 42, login: "testuser", avatar_url: "a", name: "n", iat: exp - 300, exp, kind, jti };
  const payload = btoa(JSON.stringify(claims)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  return `v1.${payload}.sig`;
}

const FRESH_STORED_ACCESS = mintToken("access", FAR_FUTURE_EXP, "stored-access");
const EXPIRED_ACCESS = mintToken("access", PAST_EXP, "expired-access");
const VALID_REFRESH = mintToken("refresh", FAR_FUTURE_EXP, "valid-refresh");
const EXPIRED_REFRESH = mintToken("refresh", PAST_EXP, "expired-refresh");
const REFRESHED_ACCESS = mintToken("access", FAR_FUTURE_EXP, "fresh-access");
const SLID_REFRESH = mintToken("refresh", FAR_FUTURE_EXP, "slid-refresh");

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

function anInMemoryStorage(access: string, refresh: string, vaultUnlockKey: string | null = null): TokenStorage {
  let accessToken: string | null = access;
  let refreshToken: string | null = refresh;
  let unlockKey: string | null = vaultUnlockKey;
  return {
    getAccess: () => accessToken,
    getRefresh: () => refreshToken,
    getVaultUnlockKey: () => unlockKey,
    set: (a: string, r: string, k: string) => {
      accessToken = a;
      refreshToken = r;
      unlockKey = k || null;
    },
    clear: () => {
      accessToken = null;
      refreshToken = null;
      unlockKey = null;
    },
  };
}

const PRESENTED_UNLOCK_KEY = "6f70.slot.presented";
const ROTATED_UNLOCK_KEY = "6f70.slot.rotated";

/** Backend whose `RefreshSession` rotates the vault unlock key it is presented, as the daemon does. */
function aRotatingRefreshBackend() {
  return anInMemoryRpcBackend().implement(AuthService, {
    refreshSession: async (req: { refreshToken?: string; vaultUnlockKey?: string }) => ({
      sessionToken: REFRESHED_ACCESS,
      refreshToken: SLID_REFRESH,
      vaultUnlockKey: req.vaultUnlockKey === PRESENTED_UNLOCK_KEY ? ROTATED_UNLOCK_KEY : "",
      user: undefined,
    }),
  });
}

/** Backend whose `RefreshSession` mints a fresh pair for a valid refresh token, else `Unauthenticated`. */
function aRefreshBackend() {
  return anInMemoryRpcBackend().implement(AuthService, {
    refreshSession: async (req: { refreshToken?: string; sessionToken?: string }) => {
      const credential = req.refreshToken ?? req.sessionToken ?? "";
      if (credential !== VALID_REFRESH) {
        throw new ConnectError("refresh token: invalid or expired", Code.Unauthenticated);
      }
      return { sessionToken: REFRESHED_ACCESS, refreshToken: SLID_REFRESH, user: undefined };
    },
  });
}

/**
 * Backend whose `RefreshSession` always fails with a transient, network-like error (server briefly
 * unreachable) rather than a definitive auth rejection — models a mobile tab waking on a flaky
 * connection.
 */
function aTransientlyFailingRefreshBackend() {
  return anInMemoryRpcBackend().implement(AuthService, {
    refreshSession: async () => {
      throw new ConnectError("connection refused", Code.Unavailable);
    },
  });
}

/** Backend whose `RefreshSession` answers with the vault state it is given, and no key. */
function aRefreshBackendReportingTheVault(vaultState: VaultState) {
  return anInMemoryRpcBackend().implement(AuthService, {
    refreshSession: async () => ({
      sessionToken: REFRESHED_ACCESS,
      refreshToken: SLID_REFRESH,
      vaultUnlockKey: "",
      vaultState,
      user: undefined,
    }),
  });
}

const THE_PASSPHRASE = "correct horse battery staple";
const UNLOCKED_UNLOCK_KEY = "6f70.slot.unlocked";

/**
 * Backend for a daemon whose vault opens under `THE_PASSPHRASE` only: `UnlockVault` and
 * `ResetVault` hand back a fresh unlock key, and a wrong passphrase is `FailedPrecondition`, as the
 * daemon refuses it. `Logout` is answered and recorded.
 */
function aVaultBackend() {
  return anInMemoryRpcBackend().implement(AuthService, {
    unlockVault: async (req: { passphrase?: string }) => {
      if (req.passphrase !== THE_PASSPHRASE) {
        throw new ConnectError("the credential vault is locked: that key does not open it", Code.FailedPrecondition);
      }
      return { vaultState: VaultState.OPEN, vaultUnlockKey: UNLOCKED_UNLOCK_KEY };
    },
    resetVault: async () => ({ vaultState: VaultState.OPEN, vaultUnlockKey: UNLOCKED_UNLOCK_KEY }),
    logout: async () => ({}),
    refreshSession: async () => ({
      sessionToken: REFRESHED_ACCESS,
      refreshToken: SLID_REFRESH,
      vaultUnlockKey: ROTATED_UNLOCK_KEY,
      vaultState: VaultState.OPEN,
      user: undefined,
    }),
  });
}

/** Backend whose `Logout` cannot be reached. */
function anUnreachableLogoutBackend() {
  return anInMemoryRpcBackend().implement(AuthService, {
    logout: async () => {
      throw new ConnectError("connection refused", Code.Unavailable);
    },
  });
}

function aStore(deps: {
  storage: TokenStorage;
  backend?: ReturnType<typeof aRefreshBackend>;
  onLoggedOut?: () => void;
  onVaultStateChange?: (state: VaultState) => void;
}) {
  const backend = deps.backend ?? aRefreshBackend();
  const authClient = createClient(AuthService, backend.transport());
  const store = createSessionTokenStore({
    authClient,
    storage: deps.storage,
    now: () => NOW_MS,
    onLoggedOut: deps.onLoggedOut,
    onVaultStateChange: deps.onVaultStateChange,
  });
  return { store, backend };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("createSessionTokenStore", () => {
  it("returns the stored access token unchanged when it is not near expiry", async () => {
    // Given — a stored access token valid far into the future
    const { store, backend } = aStore({ storage: anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH) });

    // When — a fresh access token is requested
    const token = await store.ensureFreshAccessToken();

    // Then — the stored token is returned and no refresh was performed
    expect(token).toBe(FRESH_STORED_ACCESS);
    expect(backend.callsTo(AuthService.method.refreshSession)).toHaveLength(0);
  });

  it("refreshes and returns a new access token when the stored one is expired", async () => {
    // Given — an expired access token and a valid refresh token
    const { store, backend } = aStore({ storage: anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH) });

    // When — a fresh access token is requested
    const token = await store.ensureFreshAccessToken();

    // Then — the newly minted access token is returned via exactly one refresh
    expect(token).toBe(REFRESHED_ACCESS);
    expect(backend.callsTo(AuthService.method.refreshSession)).toHaveLength(1);
  });

  it("performs exactly one refresh when several callers request a token concurrently", async () => {
    // Given — an expired access token and a valid refresh token
    const { store, backend } = aStore({ storage: anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH) });

    // When — three callers request a token at once
    const tokens = await Promise.all([
      store.ensureFreshAccessToken(),
      store.ensureFreshAccessToken(),
      store.ensureFreshAccessToken(),
    ]);

    // Then — all get the refreshed token from a single refresh call
    expect(tokens).toEqual([REFRESHED_ACCESS, REFRESHED_ACCESS, REFRESHED_ACCESS]);
    expect(backend.callsTo(AuthService.method.refreshSession)).toHaveLength(1);
  });

  it("persists both the new access and refresh tokens after a refresh", async () => {
    // Given — an expired access token and a valid refresh token
    const storage = anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH);
    const { store } = aStore({ storage });

    // When — the token is refreshed
    await store.ensureFreshAccessToken();

    // Then — the store persisted the fresh access token and the slid refresh token
    expect(storage.getAccess()).toBe(REFRESHED_ACCESS);
    expect(storage.getRefresh()).toBe(SLID_REFRESH);
  });

  it("clears both tokens and reports logged-out when the refresh token is rejected", async () => {
    // Given — an expired access token and an expired refresh token
    const storage = anInMemoryStorage(EXPIRED_ACCESS, EXPIRED_REFRESH);
    let loggedOut = 0;
    const { store } = aStore({ storage, onLoggedOut: () => (loggedOut += 1) });

    // When — a fresh access token is requested
    const attempt = store.ensureFreshAccessToken();

    // Then — the request rejects, both tokens are cleared, and logout is reported
    await expect(attempt).rejects.toThrow();
    expect(storage.getAccess()).toBe(null);
    expect(storage.getRefresh()).toBe(null);
    expect(loggedOut).toBe(1);
  });

  it("presents the vault unlock key on refresh and keeps the rotated one in its place", async () => {
    // Given — a lineage holding the unlock key its last refresh returned
    const storage = anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store, backend } = aStore({ storage, backend: aRotatingRefreshBackend() });

    // When — the token is refreshed
    await store.ensureFreshAccessToken();

    // Then — the daemon was handed the key, and the rotated one replaced it: the presented one
    // opens nothing any more
    expect(
      backend.callsTo(AuthService.method.refreshSession).map((request) => request.vaultUnlockKey),
    ).toEqual([PRESENTED_UNLOCK_KEY]);
    expect(storage.getVaultUnlockKey()).toBe(ROTATED_UNLOCK_KEY);
  });

  it("refreshes on demand even while the access token is still fresh", async () => {
    // Given — a fresh access token, as a page load after a daemon restart would find it
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store, backend } = aStore({ storage, backend: aRotatingRefreshBackend() });

    // When — the page asks for a refresh now, to reopen the operator's credentials
    await store.refreshNow();

    // Then — exactly one refresh ran, carrying the key
    expect(backend.callsTo(AuthService.method.refreshSession)).toHaveLength(1);
    expect(storage.getVaultUnlockKey()).toBe(ROTATED_UNLOCK_KEY);
  });

  it("keeps both tokens and does not report logged-out when a refresh fails transiently", async () => {
    // Given — an expired access token but a still-valid refresh token, and a server that is
    // momentarily unreachable (a mobile tab that just woke on a flaky connection).
    const storage = anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH);
    let loggedOut = 0;
    const { store } = aStore({
      storage,
      backend: aTransientlyFailingRefreshBackend(),
      onLoggedOut: () => (loggedOut += 1),
    });

    // When — a fresh access token is requested
    const attempt = store.ensureFreshAccessToken();

    // Then — it rejects, but the still-valid refresh token is preserved and logout is NOT reported:
    // a transient blip must never discard tokens (that is what forces a spurious re-login).
    await expect(attempt).rejects.toThrow();
    expect(storage.getAccess()).toBe(EXPIRED_ACCESS);
    expect(storage.getRefresh()).toBe(VALID_REFRESH);
    expect(loggedOut).toBe(0);
  });
});

describe("the vault unlock key a session lineage holds", () => {
  it("is handed to the daemon at logout, so the daemon can remove its slot", async () => {
    // Given — a signed-in lineage holding an unlock key
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store, backend } = aStore({ storage, backend: aVaultBackend() });

    // When — the operator signs out
    await store.logout();

    // Then — the daemon was sent the key
    expect(backend.callsTo(AuthService.method.logout).map((request) => request.vaultUnlockKey)).toEqual([
      PRESENTED_UNLOCK_KEY,
    ]);
  });

  it("is cleared at logout, together with both tokens", async () => {
    // Given — a signed-in lineage holding an unlock key
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store } = aStore({ storage, backend: aVaultBackend() });

    // When — the operator signs out
    await store.logout();

    // Then
    expect([storage.getAccess(), storage.getRefresh(), storage.getVaultUnlockKey()]).toEqual([null, null, null]);
  });

  it("is cleared at logout even when the daemon cannot be reached", async () => {
    // Given — a lineage holding an unlock key, and a daemon that does not answer
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store } = aStore({ storage, backend: anUnreachableLogoutBackend() });

    // When — the operator signs out
    await store.logout();

    // Then — the browser keeps nothing; a slot left behind on the daemon opens nothing without it
    expect(storage.getVaultUnlockKey()).toBe(null);
  });

  it("is cleared when the daemon refuses the refresh token as Unauthenticated", async () => {
    // Given — a lineage holding an unlock key and a refresh token the daemon no longer accepts
    const storage = anInMemoryStorage(EXPIRED_ACCESS, EXPIRED_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store } = aStore({ storage });

    // When — a fresh access token is requested
    const attempt = store.ensureFreshAccessToken();

    // Then — the session is over, and the key goes with it
    await expect(attempt).rejects.toThrow();
    expect(storage.getVaultUnlockKey()).toBe(null);
  });

  it("is removed when a refresh returns an empty one", async () => {
    // Given — a lineage holding a key the daemon can no longer open its slot with
    const storage = anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH, "6f70.slot.long-gone");
    const { store } = aStore({ storage, backend: aRotatingRefreshBackend() });

    // When — the token is refreshed, and the daemon hands back no key
    await store.ensureFreshAccessToken();

    // Then — a key that opens nothing is not kept
    expect(storage.getVaultUnlockKey()).toBe(null);
  });

  it("makes a page load refresh at once, presenting it", async () => {
    // Given — a page load holding a fresh access token and an unlock key
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH, PRESENTED_UNLOCK_KEY);
    const { store, backend } = aStore({ storage, backend: aRotatingRefreshBackend() });

    // When — the page reopens the operator's credentials
    await store.reopenVaultOnLoad();

    // Then — one refresh carried the key, so a daemon that restarted reopens the vault now
    expect(backend.callsTo(AuthService.method.refreshSession).map((request) => request.vaultUnlockKey)).toEqual([
      PRESENTED_UNLOCK_KEY,
    ]);
  });

  it("is what a page load's refresh depends on: without one, the page does not refresh", async () => {
    // Given — a page load holding a fresh access token and no unlock key
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH);
    const { store, backend } = aStore({ storage, backend: aRotatingRefreshBackend() });

    // When
    await store.reopenVaultOnLoad();

    // Then
    expect(backend.callsTo(AuthService.method.refreshSession)).toHaveLength(0);
  });
});

describe("the credential vault's state", () => {
  it("is reported from every refresh", async () => {
    // Given — a daemon that restarted, and whose vault this lineage cannot reopen
    const storage = anInMemoryStorage(EXPIRED_ACCESS, VALID_REFRESH);
    const reported: VaultState[] = [];
    const { store } = aStore({
      storage,
      backend: aRefreshBackendReportingTheVault(VaultState.LOCKED),
      onVaultStateChange: (state) => reported.push(state),
    });

    // When — the token is refreshed
    await store.ensureFreshAccessToken();

    // Then — the page learns the vault needs its passphrase
    expect(reported).toEqual([VaultState.LOCKED]);
  });

  it("becomes open when the passphrase unlocks it, and the returned key is kept", async () => {
    // Given — a signed-in lineage whose vault is locked
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH);
    const reported: VaultState[] = [];
    const { store } = aStore({
      storage,
      backend: aVaultBackend(),
      onVaultStateChange: (state) => reported.push(state),
    });

    // When — the operator gives the passphrase
    await store.unlockVault(THE_PASSPHRASE, false);

    // Then
    expect([reported, storage.getVaultUnlockKey()]).toEqual([[VaultState.OPEN], UNLOCKED_UNLOCK_KEY]);
  });

  it("is sent the passphrase with the lineage's access token", async () => {
    // Given — a signed-in lineage whose vault has not been created
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH);
    const { store, backend } = aStore({ storage, backend: aVaultBackend() });

    // When — the operator chooses a first passphrase
    await store.unlockVault(THE_PASSPHRASE, true);

    // Then
    expect(
      backend
        .callsTo(AuthService.method.unlockVault)
        .map((request) => [request.sessionToken, request.passphrase, request.create]),
    ).toEqual([[FRESH_STORED_ACCESS, THE_PASSPHRASE, true]]);
  });

  it("stays as it was when the passphrase is wrong, and the refusal reaches the caller", async () => {
    // Given — a signed-in lineage whose vault is locked
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH);
    const reported: VaultState[] = [];
    const { store } = aStore({
      storage,
      backend: aVaultBackend(),
      onVaultStateChange: (state) => reported.push(state),
    });

    // When — the wrong passphrase is given
    const attempt = store.unlockVault("not the passphrase", false);

    // Then — the prompt can say so, and nothing was stored or reported
    await expect(attempt).rejects.toThrow("locked");
    expect([reported, storage.getVaultUnlockKey(), storage.getAccess()]).toEqual([[], null, FRESH_STORED_ACCESS]);
  });

  it("becomes open under a reset, with the key to the fresh vault kept", async () => {
    // Given — a signed-in lineage that forgot its passphrase
    const storage = anInMemoryStorage(FRESH_STORED_ACCESS, VALID_REFRESH);
    const reported: VaultState[] = [];
    const { store } = aStore({
      storage,
      backend: aVaultBackend(),
      onVaultStateChange: (state) => reported.push(state),
    });

    // When — the operator resets the vault under a new passphrase
    await store.resetVault("a passphrase chosen after forgetting");

    // Then
    expect([reported, storage.getVaultUnlockKey()]).toEqual([[VaultState.OPEN], UNLOCKED_UNLOCK_KEY]);
  });
});
