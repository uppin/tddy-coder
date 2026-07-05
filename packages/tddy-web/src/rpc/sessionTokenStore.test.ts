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
import { AuthService } from "../gen/auth_pb";

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

function anInMemoryStorage(access: string, refresh: string): TokenStorage {
  let accessToken: string | null = access;
  let refreshToken: string | null = refresh;
  return {
    getAccess: () => accessToken,
    getRefresh: () => refreshToken,
    set: (a: string, r: string) => {
      accessToken = a;
      refreshToken = r;
    },
    clear: () => {
      accessToken = null;
      refreshToken = null;
    },
  };
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

function aStore(deps: {
  storage: TokenStorage;
  backend?: ReturnType<typeof aRefreshBackend>;
  onLoggedOut?: () => void;
}) {
  const backend = deps.backend ?? aRefreshBackend();
  const authClient = createClient(AuthService, backend.transport());
  const store = createSessionTokenStore({
    authClient,
    storage: deps.storage,
    now: () => NOW_MS,
    onLoggedOut: deps.onLoggedOut,
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
});
