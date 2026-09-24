/**
 * In-memory `auth.AuthService` backend for the **redirect-flow** sign-in: the operator returns from
 * GitHub to `/auth/callback?code=…&state=…` and the page trades the code for a session with
 * `ExchangeCode`.
 *
 * `ExchangeCode` answers with one scripted response, recorded by the testkit so a test can read the
 * code and state each exchange presented. The session it mints is the same triple an approved
 * device login carries, so both flows' fixtures share the approving operator and its tokens.
 *
 * Proto: packages/tddy-service/proto/auth.proto
 */

import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { create } from "@bufbuild/protobuf";
import { AuthService, ExchangeCodeResponseSchema, type ExchangeCodeResponse } from "../../../src/gen/auth_pb";
import {
  DEVICE_LOGIN_ACCESS_TOKEN,
  DEVICE_LOGIN_REFRESH_TOKEN,
  theApprovingOperator,
  type CompletedSessionPart,
} from "./deviceLoginBackend";

export { ACCESS_TOKEN_KEY, REFRESH_TOKEN_KEY } from "./durableSessionBackend";
export { theApprovingOperator, type CompletedSessionPart } from "./deviceLoginBackend";

/** The access token a code exchange mints (a real, far-future `v1.` token). */
export const EXCHANGED_ACCESS_TOKEN = DEVICE_LOGIN_ACCESS_TOKEN;
/** The refresh token a code exchange mints alongside the access token. */
export const EXCHANGED_REFRESH_TOKEN = DEVICE_LOGIN_REFRESH_TOKEN;

/** The authorization code GitHub hands back on the callback URL. */
export const GITHUB_AUTHORIZATION_CODE = "e72e16c7e42f292c6912";
/** The OAuth `state` GitHub echoes back on the callback URL. */
export const OAUTH_STATE = "b7c1f0d2-9a3e-4c5b-8f6a-1d2e3f4a5b6c";

// ---------------------------------------------------------------------------
// Exchanges — what ExchangeCode returns for the code
// ---------------------------------------------------------------------------

/** A whole session, as `ExchangeCode` returns it for a valid code. */
export function aCodeExchange(): ExchangeCodeResponse {
  return create(ExchangeCodeResponseSchema, {
    sessionToken: EXCHANGED_ACCESS_TOKEN,
    refreshToken: EXCHANGED_REFRESH_TOKEN,
    user: theApprovingOperator(),
  });
}

/**
 * A code exchange without `missing` — the user absent, or a token empty, as proto3 leaves a field
 * the daemon never set. Every other part is exactly what {@link aCodeExchange} carries.
 */
export function aCodeExchangeMissing(missing: CompletedSessionPart): ExchangeCodeResponse {
  const absent: Record<CompletedSessionPart, Partial<ExchangeCodeResponse>> = {
    user: { user: undefined },
    sessionToken: { sessionToken: "" },
    refreshToken: { refreshToken: "" },
  };
  return create(ExchangeCodeResponseSchema, {
    sessionToken: EXCHANGED_ACCESS_TOKEN,
    refreshToken: EXCHANGED_REFRESH_TOKEN,
    user: theApprovingOperator(),
    ...absent[missing],
  });
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

/** An `auth.AuthService` backend whose `ExchangeCode` answers every code with `exchange`. */
export function aRedirectLoginBackend(exchange: ExchangeCodeResponse): InMemoryRpcBackend {
  return anInMemoryRpcBackend().implement(AuthService, {
    exchangeCode: async () => exchange,
    getAuthStatus: async (req) =>
      req.sessionToken === EXCHANGED_ACCESS_TOKEN
        ? { authenticated: true, user: theApprovingOperator() }
        : { authenticated: false, user: undefined },
  });
}
