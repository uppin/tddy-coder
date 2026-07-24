import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { AuthService } from "../gen/auth_pb";
import type { GitHubUser } from "../gen/auth_pb";
import { useHttpClient, useAuthTokenGate } from "../rpc/transportProvider";
import { createSessionTokenStore, type TokenStorage } from "../rpc/sessionTokenStore";

/** Thrown when the server authoritatively reports the stored session is not valid (vs. a transient
 * network failure). Distinguishing the two is what stops a momentary blip from logging the user out. */
class SessionInvalidError extends Error {}

/** Short-lived access token used to authenticate RPCs. */
const ACCESS_TOKEN_KEY = "tddy_session_token";
/** Long-lived refresh token used only to mint fresh access tokens. */
const REFRESH_TOKEN_KEY = "tddy_refresh_token";
const OAUTH_STATE_KEY = "tddy_oauth_state";
export const OAUTH_RETURN_TO_KEY = "tddy_oauth_return_to";

export interface AuthState {
  user: GitHubUser | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
  /** Access token for RPC calls (when authenticated). */
  sessionToken: string | null;
  /** True while the session store is minting a fresh access token from the refresh token. */
  isRefreshing: boolean;
}

const LOGGED_OUT: AuthState = {
  user: null,
  isAuthenticated: false,
  isLoading: false,
  error: null,
  sessionToken: null,
  isRefreshing: false,
};

/** `localStorage`-backed persistence for the access + refresh token pair. */
function localStorageTokenStorage(): TokenStorage {
  return {
    getAccess: () => localStorage.getItem(ACCESS_TOKEN_KEY),
    getRefresh: () => localStorage.getItem(REFRESH_TOKEN_KEY),
    set: (access, refresh) => {
      localStorage.setItem(ACCESS_TOKEN_KEY, access);
      localStorage.setItem(REFRESH_TOKEN_KEY, refresh);
    },
    clear: () => {
      localStorage.removeItem(ACCESS_TOKEN_KEY);
      localStorage.removeItem(REFRESH_TOKEN_KEY);
    },
  };
}

export function useAuth() {
  const client = useHttpClient(AuthService);
  const authTokenGate = useAuthTokenGate();
  const [state, setState] = useState<AuthState>({ ...LOGGED_OUT, isLoading: true });

  // One stable storage instance for this hook's lifetime.
  const storageRef = useRef<TokenStorage | null>(null);
  storageRef.current ??= localStorageTokenStorage();
  const storage = storageRef.current;

  // The single session-token store. Owns the access+refresh pair, decides when to refresh, and
  // performs the refresh single-flight. Rebuilt only if the auth client changes.
  const store = useMemo(
    () =>
      createSessionTokenStore({
        authClient: client,
        storage,
        onLoggedOut: () => setState(LOGGED_OUT),
        onRefreshingChange: (refreshing) => setState((s) => ({ ...s, isRefreshing: refreshing })),
        onAccessTokenChange: (accessToken) => setState((s) => ({ ...s, sessionToken: accessToken })),
      }),
    [client, storage],
  );

  // Wire the transport's auth gate to this store, so every RPC issued through the shared transport
  // carries a request-time-fresh access token (refreshing single-flight when it has lapsed).
  useEffect(() => {
    authTokenGate.current = () => store.ensureFreshAccessToken();
    return () => {
      authTokenGate.current = null;
    };
  }, [authTokenGate, store]);

  // On mount: resolve a live session from the stored tokens. A stored access token is validated
  // as-is first; only if it is rejected (e.g. expired after a long sleep) and a refresh token
  // exists is a fresh access token minted from it. Just a rejected refresh logs the user out.
  useEffect(() => {
    if (!storage.getAccess() && !storage.getRefresh()) {
      setState(LOGGED_OUT);
      return;
    }
    let cancelled = false;

    const establishSession = async (): Promise<{ token: string; user: GitHubUser }> => {
      const access = storage.getAccess();
      if (access) {
        const status = await client.getAuthStatus({ sessionToken: access });
        if (status.authenticated && status.user) {
          // The transport's auth gate may have refreshed the token mid-call (an expired token is
          // re-minted on the way out), so report the authoritative stored token, not the local one.
          return { token: storage.getAccess() ?? access, user: status.user };
        }
      }
      if (storage.getRefresh()) {
        const token = await store.ensureFreshAccessToken();
        if (token) {
          const status = await client.getAuthStatus({ sessionToken: token });
          if (status.authenticated && status.user) {
            return { token, user: status.user };
          }
        }
      }
      // Reached the server and it did not authenticate us — a definitive invalid session.
      throw new SessionInvalidError();
    };

    // A definitive auth rejection ends the session; a transient/network failure must not. A
    // just-woken mobile tab often reloads while its connection is briefly unavailable, so the first
    // establish attempt can fail purely on connectivity — treating that as "logged out" (and wiping
    // the valid 7-day refresh token) is the bug that forces a re-login. Only these are terminal:
    //   • the server said the session is invalid (SessionInvalidError),
    //   • an RPC was explicitly rejected as Unauthenticated,
    //   • the store already cleared both tokens (nothing left to recover from).
    const isTerminal = (err: unknown) =>
      err instanceof SessionInvalidError ||
      ConnectError.from(err).code === Code.Unauthenticated ||
      (!storage.getAccess() && !storage.getRefresh());

    const run = async () => {
      // Backoff between retries of a transient failure; stays in the "Loading…" state throughout,
      // so the operator sees a brief spinner rather than a spurious login screen.
      const backoffsMs = [500, 1000, 2000, 4000];
      for (let attempt = 0; ; attempt++) {
        try {
          const { token, user } = await establishSession();
          if (cancelled) return;
          setState({
            user,
            isAuthenticated: true,
            isLoading: false,
            error: null,
            sessionToken: token,
            isRefreshing: false,
          });
          return;
        } catch (err) {
          if (cancelled) return;
          if (isTerminal(err) || attempt >= backoffsMs.length) {
            // Wipe tokens only on a definitive rejection. When transient retries are merely
            // exhausted, keep the tokens so a later reload/resume can still recover the session.
            if (isTerminal(err)) storage.clear();
            setState(LOGGED_OUT);
            return;
          }
          await new Promise((resolve) => setTimeout(resolve, backoffsMs[attempt]));
        }
      }
    };
    void run();

    return () => {
      cancelled = true;
    };
  }, [client, store, storage]);

  // Proactively refresh the access token when the tab returns to the foreground or the network
  // comes back. Mobile browsers freeze and often discard backgrounded tabs, so the short-lived
  // access token lapses while hidden with no RPC running to refresh it reactively. Refreshing on
  // resume keeps the session alive and primes a valid token before the first RPC fires on a
  // flaky wake-up connection. A no-op when the token is still fresh; failures are swallowed (a
  // definitive logout is handled by the store's onLoggedOut, a transient one retries later).
  useEffect(() => {
    const refreshOnResume = () => {
      if (typeof document !== "undefined" && document.visibilityState === "hidden") return;
      void store.ensureFreshAccessToken().catch(() => {});
    };
    document.addEventListener("visibilitychange", refreshOnResume);
    window.addEventListener("pageshow", refreshOnResume);
    window.addEventListener("online", refreshOnResume);
    return () => {
      document.removeEventListener("visibilitychange", refreshOnResume);
      window.removeEventListener("pageshow", refreshOnResume);
      window.removeEventListener("online", refreshOnResume);
    };
  }, [store]);

  const login = useCallback(
    async (returnTo?: string) => {
      try {
        const res = await client.getAuthUrl({});
        sessionStorage.setItem(OAUTH_STATE_KEY, res.state);
        if (returnTo && returnTo !== "/") {
          sessionStorage.setItem(OAUTH_RETURN_TO_KEY, returnTo);
        } else {
          sessionStorage.removeItem(OAUTH_RETURN_TO_KEY);
        }
        window.location.href = res.authorizeUrl;
      } catch (e) {
        setState((s) => ({
          ...s,
          error: e instanceof Error ? e.message : "Failed to get auth URL",
        }));
      }
    },
    [client],
  );

  const handleCallback = useCallback(
    async (code: string, state: string) => {
      const storedState = sessionStorage.getItem(OAUTH_STATE_KEY);
      if (storedState && storedState !== state) {
        setState((s) => ({ ...s, isLoading: false, error: "OAuth state mismatch" }));
        return;
      }
      sessionStorage.removeItem(OAUTH_STATE_KEY);
      try {
        const res = await client.exchangeCode({ code, state });
        storage.set(res.sessionToken, res.refreshToken);
        setState({
          user: res.user ?? null,
          isAuthenticated: true,
          isLoading: false,
          error: null,
          sessionToken: res.sessionToken,
          isRefreshing: false,
        });
      } catch (e) {
        storage.clear();
        setState({
          ...LOGGED_OUT,
          error: e instanceof Error ? e.message : "Code exchange failed",
        });
      }
    },
    [client, storage],
  );

  const logout = useCallback(async () => {
    const token = storage.getAccess();
    if (token) {
      try {
        await client.logout({ sessionToken: token });
      } catch {
        // Ignore logout errors — the client discards its tokens regardless.
      }
    }
    storage.clear();
    setState(LOGGED_OUT);
  }, [client, storage]);

  return { ...state, login, handleCallback, logout };
}
