/**
 * ConnectRPC interceptor that gates every RPC behind a request-time-fresh access token.
 *
 * The session (access) token is carried as a `sessionToken` field on request messages. Before
 * forwarding such a request, this interceptor awaits `ensureFreshAccessToken()` — which refreshes
 * the token single-flight when it has lapsed — and rewrites the field, so a call issued right after
 * the device wakes waits for the refresh and is sent with the fresh token rather than the stale one.
 *
 * `RefreshSession`'s own request carries a `refreshToken` (not a `sessionToken`), so it is naturally
 * exempt and cannot recurse through the gate.
 */

import type { Interceptor } from "@connectrpc/connect";

/** True when `message` owns a string `sessionToken` field the gate should refresh. */
export function carriesSessionToken(message: unknown): message is { sessionToken: string } {
  return (
    typeof message === "object" &&
    message !== null &&
    "sessionToken" in message &&
    typeof (message as { sessionToken: unknown }).sessionToken === "string"
  );
}

/**
 * Build the auth-gate interceptor. `ensureFreshAccessToken` is the single-flight token resolver
 * (see {@link createSessionTokenStore}); the interceptor calls it per request rather than closing
 * over a token so it always sends the currently-valid one. Resolving to `null` means "no token
 * available to inject" — the request's own `sessionToken` is left as-is (used in production before
 * any auth provider has installed a resolver).
 */
export function createAuthGateInterceptor(
  ensureFreshAccessToken: () => Promise<string | null>,
): Interceptor {
  return (next) => async (req) => {
    if (!req.stream && carriesSessionToken(req.message)) {
      const token = await ensureFreshAccessToken();
      if (token !== null) {
        req.message.sessionToken = token;
      }
    }
    return next(req);
  };
}
