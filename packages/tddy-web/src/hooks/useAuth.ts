import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { AuthService, DeviceLoginState } from "../gen/auth_pb";
import type { GitHubUser, PollDeviceLoginResponse } from "../gen/auth_pb";
import { useHttpClient, useAuthTokenGate } from "../rpc/transportProvider";
import { createSessionTokenStore, type TokenStorage } from "../rpc/sessionTokenStore";

/** Thrown when the server authoritatively reports the stored session is not valid (vs. a transient
 * network failure). Distinguishing the two is what stops a momentary blip from logging the user out. */
class SessionInvalidError extends Error {}

/** Short-lived access token used to authenticate RPCs. */
const ACCESS_TOKEN_KEY = "tddy_session_token";
/** Long-lived refresh token used only to mint fresh access tokens. */
const REFRESH_TOKEN_KEY = "tddy_refresh_token";
/**
 * The daemon's vault unlock key for this session lineage — a wrap key, not a stored credential.
 * Alone it opens nothing; kept beside the refresh token so a refresh can reopen the operator's
 * credentials after a daemon restart.
 */
const VAULT_UNLOCK_KEY_KEY = "tddy_vault_unlock_key";
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

/**
 * Where a device-flow sign-in (`StartDeviceLogin` / `PollDeviceLogin`) stands.
 *
 * `awaiting-approval` carries what the operator needs to approve the attempt at GitHub. `denied`
 * and `expired` are distinct because the operator's next move differs — a refusal was theirs, an
 * expiry was the clock's — though both end the attempt and are left by starting a fresh one.
 * `failed` is an attempt that ended on an error the daemon returned rather than on GitHub's answer.
 */
export type DeviceLogin =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "awaiting-approval"; userCode: string; verificationUri: string }
  | { phase: "denied" }
  | { phase: "expired" }
  | { phase: "failed"; error: string };

const DEVICE_LOGIN_IDLE: DeviceLogin = { phase: "idle" };

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
    getVaultUnlockKey: () => localStorage.getItem(VAULT_UNLOCK_KEY_KEY),
    set: (access, refresh, vaultUnlockKey) => {
      localStorage.setItem(ACCESS_TOKEN_KEY, access);
      localStorage.setItem(REFRESH_TOKEN_KEY, refresh);
      if (vaultUnlockKey) {
        localStorage.setItem(VAULT_UNLOCK_KEY_KEY, vaultUnlockKey);
      } else {
        localStorage.removeItem(VAULT_UNLOCK_KEY_KEY);
      }
    },
    clear: () => {
      localStorage.removeItem(ACCESS_TOKEN_KEY);
      localStorage.removeItem(REFRESH_TOKEN_KEY);
      localStorage.removeItem(VAULT_UNLOCK_KEY_KEY);
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
          // A daemon that restarted since this lineage last refreshed holds none of the operator's
          // credentials open until a refresh presents the unlock key — do that now rather than
          // when the access token next lapses. A failure is the store's to report, not a logout.
          if (storage.getVaultUnlockKey() && storage.getRefresh()) {
            void store.refreshNow().catch(() => {});
          }
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

  // Take up a session the daemon minted — by `ExchangeCode` or by an approved device login, which
  // return the same fields. Both flows store it here, so both leave the operator signed in alike.
  const adoptSession = useCallback(
    (
      sessionToken: string,
      refreshToken: string,
      vaultUnlockKey: string,
      user: GitHubUser | undefined,
    ) => {
      storage.set(sessionToken, refreshToken, vaultUnlockKey);
      setState({
        user: user ?? null,
        isAuthenticated: true,
        isLoading: false,
        error: null,
        sessionToken,
        isRefreshing: false,
      });
    },
    [storage],
  );

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
        adoptSession(res.sessionToken, res.refreshToken, res.vaultUnlockKey, res.user);
      } catch (e) {
        storage.clear();
        setState({
          ...LOGGED_OUT,
          error: e instanceof Error ? e.message : "Code exchange failed",
        });
      }
    },
    [client, storage, adoptSession],
  );

  // The device-flow attempt in progress. Only the latest attempt may act on an answer: starting
  // again (or unmounting) bumps `generation`, so a poll still in flight for the old device code
  // lands on a dead attempt and schedules nothing.
  const [deviceLogin, setDeviceLogin] = useState<DeviceLogin>(DEVICE_LOGIN_IDLE);
  const deviceAttemptRef = useRef<{ generation: number; timer: ReturnType<typeof setTimeout> | null }>({
    generation: 0,
    timer: null,
  });

  const endDeviceAttempt = useCallback(() => {
    const attempt = deviceAttemptRef.current;
    attempt.generation += 1;
    if (attempt.timer !== null) clearTimeout(attempt.timer);
    attempt.timer = null;
  }, []);

  useEffect(() => endDeviceAttempt, [endDeviceAttempt]);

  const startDeviceLogin = useCallback(async () => {
    endDeviceAttempt();
    const attempt = deviceAttemptRef.current;
    const generation = attempt.generation;
    const isCurrent = () => attempt.generation === generation;
    const fail = (e: unknown, defaultMessage: string) => {
      if (!isCurrent()) return;
      setDeviceLogin({ phase: "failed", error: e instanceof Error ? e.message : defaultMessage });
    };

    setDeviceLogin({ phase: "starting" });
    let grant;
    try {
      grant = await client.startDeviceLogin({});
    } catch (e) {
      fail(e, "Failed to start device sign-in");
      return;
    }
    if (!isCurrent()) return;

    const { deviceCode } = grant;
    // GitHub's floor between polls. A slow-down answer raises it for every later poll.
    let intervalMs = Number(grant.intervalSeconds) * 1000;

    const answered = (res: PollDeviceLoginResponse) => {
      switch (res.state) {
        case DeviceLoginState.PENDING:
          scheduleNextPoll();
          return;
        case DeviceLoginState.SLOW_DOWN:
          intervalMs = Number(res.intervalSeconds) * 1000;
          scheduleNextPoll();
          return;
        case DeviceLoginState.COMPLETE:
          setDeviceLogin(DEVICE_LOGIN_IDLE);
          adoptSession(res.sessionToken, res.refreshToken, res.vaultUnlockKey, res.user);
          return;
        case DeviceLoginState.DENIED:
          setDeviceLogin({ phase: "denied" });
          return;
        case DeviceLoginState.EXPIRED:
          setDeviceLogin({ phase: "expired" });
          return;
        default:
          setDeviceLogin({ phase: "failed", error: `Unrecognised device sign-in state ${res.state}` });
      }
    };

    const poll = async () => {
      attempt.timer = null;
      let res;
      try {
        res = await client.pollDeviceLogin({ deviceCode });
      } catch (e) {
        fail(e, "Device sign-in failed");
        return;
      }
      if (isCurrent()) answered(res);
    };

    const scheduleNextPoll = () => {
      attempt.timer = setTimeout(() => void poll(), intervalMs);
    };

    setDeviceLogin({
      phase: "awaiting-approval",
      userCode: grant.userCode,
      verificationUri: grant.verificationUri,
    });
    scheduleNextPoll();
  }, [client, adoptSession, endDeviceAttempt]);

  const logout = useCallback(async () => {
    const token = storage.getAccess();
    // Handed back so the daemon removes this lineage's vault unlock slot — even when the access
    // token has lapsed, since the key itself identifies the slot.
    const vaultUnlockKey = storage.getVaultUnlockKey() ?? "";
    if (token || vaultUnlockKey) {
      try {
        await client.logout({ sessionToken: token ?? "", vaultUnlockKey });
      } catch {
        // Ignore logout errors — the client discards its tokens regardless.
      }
    }
    storage.clear();
    setState(LOGGED_OUT);
  }, [client, storage]);

  return { ...state, login, handleCallback, logout, deviceLogin, startDeviceLogin };
}
