/**
 * In-memory `auth.AuthService` backend for the two-token durable-session lifecycle
 * (access token + long-lived refresh token). Companion to `authRefreshBackend.ts`, which only
 * models the single-token refresh; this one models the target State B from
 * `docs/dev/1-WIP/2026-07-04-durable-web-session.md`.
 *
 * Tokens are real `v1.<base64url(payload)>.<sig>` strings so the production client can decode the
 * payload's `exp`/`kind` exactly as it will against a live daemon (the signature is never checked
 * client-side). `RefreshSession` mirrors the server contract: it accepts a *refresh*-kind,
 * unexpired token and returns a fresh access token plus a slid refresh token; anything else is
 * rejected `Unauthenticated`.
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ConnectError, Code } from "@connectrpc/connect";
import { AuthService } from "../../../src/gen/auth_pb";
import { ConnectionService } from "../../../src/gen/connection_pb";
import { aGitHubUser } from "./responses";

type TokenKind = "access" | "refresh";

/** Far-future expiry (year 2100) — a token that is unambiguously not expired. */
const FAR_FUTURE_EXP = 4102444800;
/** Long-past expiry — a token that is unambiguously expired. */
const PAST_EXP = 1000;

function base64UrlEncode(input: string): string {
  return btoa(input).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64UrlDecode(input: string): string {
  const padded = input + "=".repeat((4 - (input.length % 4)) % 4);
  return atob(padded.replace(/-/g, "+").replace(/_/g, "/"));
}

/** Mint a `v1.` token with the given kind, expiry, and a unique `jti` so distinct tokens differ. */
function mintToken(kind: TokenKind, exp: number, jti: string): string {
  const claims = {
    id: 42,
    login: "testuser",
    avatar_url: "https://github.com/testuser.png",
    name: "Test User",
    iat: exp - 300,
    exp,
    kind,
    jti,
  };
  return `v1.${base64UrlEncode(JSON.stringify(claims))}.sig`;
}

interface DecodedClaims {
  kind: TokenKind;
  exp: number;
}

/** Decode a token's payload (no signature check) — returns null for a malformed token. */
function decodeClaims(token: string): DecodedClaims | null {
  const parts = token.split(".");
  if (parts.length !== 3 || parts[0] !== "v1") {
    return null;
  }
  const payload = JSON.parse(base64UrlDecode(parts[1])) as DecodedClaims;
  return { kind: payload.kind, exp: payload.exp };
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

// ---------------------------------------------------------------------------
// The fixed tokens a test drives the lifecycle with.
// ---------------------------------------------------------------------------

/** A currently-valid access token — a session opened well within the access-token window. */
export const CURRENT_ACCESS_TOKEN = mintToken("access", FAR_FUTURE_EXP, "current-access");
/** An access token that has already expired — the "phone was asleep" starting state. */
export const EXPIRED_ACCESS_TOKEN = mintToken("access", PAST_EXP, "expired-access");
/** A refresh token still well within its 7-day window. */
export const VALID_REFRESH_TOKEN = mintToken("refresh", FAR_FUTURE_EXP, "initial-refresh");
/** A refresh token whose 7-day window has lapsed — forces re-login. */
export const EXPIRED_REFRESH_TOKEN = mintToken("refresh", PAST_EXP, "expired-refresh");
/** The access token `RefreshSession` mints in exchange for a valid refresh token. */
export const REFRESHED_ACCESS_TOKEN = mintToken("access", FAR_FUTURE_EXP, "fresh-access");
/** The slid refresh token `RefreshSession` returns alongside the new access token. */
export const SLID_REFRESH_TOKEN = mintToken("refresh", FAR_FUTURE_EXP, "fresh-refresh");

/** `localStorage` keys the client uses for the two tokens. */
export const ACCESS_TOKEN_KEY = "tddy_session_token";
export const REFRESH_TOKEN_KEY = "tddy_refresh_token";

// ---------------------------------------------------------------------------
// Backends
// ---------------------------------------------------------------------------

/** Read the refresh credential from the request, tolerant of the pre-/post-proto-regen field name. */
function refreshCredential(req: { refreshToken?: string; sessionToken?: string }): string {
  return req.refreshToken ?? req.sessionToken ?? "";
}

function assertRefreshableOrThrow(token: string): void {
  const claims = decodeClaims(token);
  if (!claims || claims.kind !== "refresh" || claims.exp <= nowSeconds()) {
    throw new ConnectError("refresh token: invalid or expired", Code.Unauthenticated);
  }
}

/**
 * Backend implementing the two-token contract. `RefreshSession` returns
 * {@link REFRESHED_ACCESS_TOKEN} + {@link SLID_REFRESH_TOKEN} for a valid refresh token, and
 * rejects an access-kind, expired, or malformed token with `Unauthenticated`.
 */
export function aDurableSessionBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .implement(AuthService, {
      getAuthStatus: async (req) => {
        const claims = decodeClaims(req.sessionToken);
        const authenticated = !!claims && claims.kind === "access" && claims.exp > nowSeconds();
        return authenticated
          ? { authenticated: true, user: aGitHubUser() }
          : { authenticated: false, user: undefined };
      },
      refreshSession: async (req) => {
        assertRefreshableOrThrow(refreshCredential(req));
        return {
          sessionToken: REFRESHED_ACCESS_TOKEN,
          refreshToken: SLID_REFRESH_TOKEN,
          user: aGitHubUser(),
        };
      },
    })
    .implement(ConnectionService, {
      listSessions: async () => ({ sessions: [] }),
    });
}

/**
 * Like {@link aDurableSessionBackend} but holds `RefreshSession` open until `releaseRefresh()` is
 * called — lets a test observe the in-flight "refreshing" indicator before the refresh completes.
 */
export function aDeferredRefreshSessionBackend(): {
  backend: InMemoryRpcBackend;
  releaseRefresh: () => void;
} {
  let release = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const backend = anInMemoryRpcBackend()
    .implement(AuthService, {
      getAuthStatus: async (req) => {
        const claims = decodeClaims(req.sessionToken);
        const authenticated = !!claims && claims.kind === "access" && claims.exp > nowSeconds();
        return authenticated
          ? { authenticated: true, user: aGitHubUser() }
          : { authenticated: false, user: undefined };
      },
      refreshSession: async (req) => {
        assertRefreshableOrThrow(refreshCredential(req));
        await gate;
        return {
          sessionToken: REFRESHED_ACCESS_TOKEN,
          refreshToken: SLID_REFRESH_TOKEN,
          user: aGitHubUser(),
        };
      },
    })
    .implement(ConnectionService, {
      listSessions: async () => ({ sessions: [] }),
    });
  return { backend, releaseRefresh: () => release() };
}
