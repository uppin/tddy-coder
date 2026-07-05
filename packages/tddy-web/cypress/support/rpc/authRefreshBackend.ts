/**
 * In-memory `auth.AuthService` backend for the shared session-token lifecycle
 * (`AuthProvider`/`useAuthContext`).
 *
 * The session lifecycle moved to the two-token model (short-lived access token + long-lived
 * refresh token), so this is a thin alias of {@link aDurableSessionBackend} — `RefreshSession`
 * consumes a refresh token and returns a fresh access token plus a slid refresh token. Kept as a
 * named export so existing specs keep a stable import while sharing one backend implementation.
 */

export {
  aDurableSessionBackend as anAuthRefreshBackend,
  CURRENT_ACCESS_TOKEN,
  EXPIRED_ACCESS_TOKEN,
  VALID_REFRESH_TOKEN,
  REFRESHED_ACCESS_TOKEN,
  ACCESS_TOKEN_KEY,
  REFRESH_TOKEN_KEY,
} from "./durableSessionBackend";
