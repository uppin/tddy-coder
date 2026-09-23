import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { Code, ConnectError, type Client } from "@connectrpc/connect";
import { AuthService, DeviceLoginState, VaultState } from "../gen/auth_pb";
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
  /**
   * Where the operator's credential vault stands on the daemon. `LOCKED` and `UNINITIALIZED` are
   * signed in all the same; the page asks for the passphrase (`CredentialVaultPrompt`).
   */
  vaultState: VaultState;
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

/** A session as the daemon mints it — by `ExchangeCode` or by an approved device login. */
interface MintedSession {
  sessionToken: string;
  refreshToken: string;
  vaultUnlockKey: string;
  vaultState: VaultState;
  user?: GitHubUser;
}

/** A minted session with every part present — the only kind the page takes up. */
interface WholeSession {
  sessionToken: string;
  refreshToken: string;
  /** The unlock key for this lineage's credential-vault slot — `""` when the daemon keeps none. */
  vaultUnlockKey: string;
  /** Where the operator's credential vault stands after this sign-in. */
  vaultState: VaultState;
  user: GitHubUser;
}

type SessionCheck = { whole: WholeSession } | { missing: string[] };

/**
 * Whether `minted` is a whole session, or which of its parts the daemon left out. proto3 leaves a
 * field the daemon never set as an absent message or an empty string; either is a missing part,
 * never one to fill in with a default.
 */
function checkWholeSession({
  sessionToken,
  refreshToken,
  vaultUnlockKey,
  vaultState,
  user,
}: MintedSession): SessionCheck {
  const missing = [
    ...(user === undefined ? ["user"] : []),
    ...(sessionToken === "" ? ["session token"] : []),
    ...(refreshToken === "" ? ["refresh token"] : []),
  ];
  if (user === undefined || missing.length > 0) return { missing };
  // An empty unlock key is not a missing part: a stub login, or a daemon with no vault, keeps none.
  return { whole: { sessionToken, refreshToken, vaultUnlockKey, vaultState, user } };
}

/** The error for a flow that `completed` without the session parts named in `missing`. */
function noWholeSessionMessage(completed: string, missing: string[]): string {
  return `${completed} without a whole session (no ${missing.join(", no ")})`;
}

/** Milliseconds per second: the daemon names poll intervals in seconds, `setTimeout` takes ms. */
const MS_PER_SECOND = 1000;

/**
 * `seconds` — an interval the daemon named — as a delay between polls, or `null` when it is not a
 * positive interval. That is a protocol error, never a reason to poll with no delay.
 */
function intervalMsOf(seconds: bigint): number | null {
  const n = Number(seconds);
  return n > 0 ? n * MS_PER_SECOND : null;
}

/**
 * What one poll's answer tells a device-flow attempt to do next: poll again after `afterMs` (every
 * later poll keeps that delay), end on `deviceLogin`, or take up the whole session it carries.
 */
type DevicePollStep =
  | { next: "poll"; afterMs: number }
  | { next: "settle"; deviceLogin: DeviceLogin }
  | { next: "adopt"; session: WholeSession };

function deviceLoginFailed(error: string): DevicePollStep {
  return { next: "settle", deviceLogin: { phase: "failed", error } };
}

/** The step `res` calls for, for an attempt currently polling every `intervalMs`. */
function devicePollStep(res: PollDeviceLoginResponse, intervalMs: number): DevicePollStep {
  switch (res.state) {
    case DeviceLoginState.PENDING:
      return { next: "poll", afterMs: intervalMs };
    case DeviceLoginState.SLOW_DOWN: {
      const widenedMs = intervalMsOf(res.intervalSeconds);
      if (widenedMs === null) {
        return deviceLoginFailed("The daemon asked to slow down device sign-in without naming an interval");
      }
      return { next: "poll", afterMs: widenedMs };
    }
    case DeviceLoginState.COMPLETE: {
      const session = checkWholeSession(res);
      if ("missing" in session) {
        return deviceLoginFailed(noWholeSessionMessage("The daemon completed device sign-in", session.missing));
      }
      return { next: "adopt", session: session.whole };
    }
    case DeviceLoginState.DENIED:
      return { next: "settle", deviceLogin: { phase: "denied" } };
    case DeviceLoginState.EXPIRED:
      return { next: "settle", deviceLogin: { phase: "expired" } };
    default:
      return deviceLoginFailed(`Unrecognised device sign-in state ${res.state}`);
  }
}

/** An attempt that ended on `e`, worded by `e` itself when it is an `Error`, else `defaultMessage`. */
function deviceLoginError(e: unknown, defaultMessage: string): DeviceLogin {
  return { phase: "failed", error: e instanceof Error ? e.message : defaultMessage };
}

/** The device-flow attempt in progress: its generation, and the timer of its next poll. */
interface DeviceAttemptSlot {
  generation: number;
  timer: ReturnType<typeof setTimeout> | null;
}

/**
 * What one device-flow attempt's poll loop needs: the code it polls for, the slot that holds its
 * timer, whether it is still the latest attempt, and where its answers land.
 */
interface DevicePollLoop {
  client: Client<typeof AuthService>;
  deviceCode: string;
  attempt: DeviceAttemptSlot;
  isCurrent: () => boolean;
  setDeviceLogin: (deviceLogin: DeviceLogin) => void;
  adoptSession: (session: WholeSession) => void;
}

/** Poll again after `intervalMs` — the delay every later poll keeps until a slow-down widens it. */
function scheduleDevicePoll(loop: DevicePollLoop, intervalMs: number): void {
  loop.attempt.timer = setTimeout(() => void pollDeviceOnce(loop, intervalMs), intervalMs);
}

/** One poll; its answer acts only while the attempt is still the latest. */
async function pollDeviceOnce(loop: DevicePollLoop, intervalMs: number): Promise<void> {
  loop.attempt.timer = null;
  let res;
  try {
    res = await loop.client.pollDeviceLogin({ deviceCode: loop.deviceCode });
  } catch (e) {
    if (loop.isCurrent()) loop.setDeviceLogin(deviceLoginError(e, "Device sign-in failed"));
    return;
  }
  if (loop.isCurrent()) actOnDevicePollStep(loop, devicePollStep(res, intervalMs));
}

/** Carry out `step`: schedule the next poll, end the attempt, or take up its session. */
function actOnDevicePollStep(loop: DevicePollLoop, step: DevicePollStep): void {
  switch (step.next) {
    case "poll":
      scheduleDevicePoll(loop, step.afterMs);
      return;
    case "settle":
      loop.setDeviceLogin(step.deviceLogin);
      return;
    case "adopt":
      loop.setDeviceLogin(DEVICE_LOGIN_IDLE);
      loop.adoptSession(step.session);
      return;
  }
}

const LOGGED_OUT: AuthState = {
  user: null,
  isAuthenticated: false,
  isLoading: false,
  error: null,
  sessionToken: null,
  isRefreshing: false,
  vaultState: VaultState.UNSPECIFIED,
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

/** Signed in as `user`, authenticating RPCs with `sessionToken`. */
function signedInState(user: GitHubUser, sessionToken: string, vaultState: VaultState): AuthState {
  return {
    user,
    isAuthenticated: true,
    isLoading: false,
    error: null,
    sessionToken,
    isRefreshing: false,
    vaultState,
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
        onVaultStateChange: (vaultState) => setState((s) => ({ ...s, vaultState })),
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

    const establishSession = async (): Promise<{ token: string; user: GitHubUser; vaultState: VaultState }> => {
      const access = storage.getAccess();
      if (access) {
        const status = await client.getAuthStatus({ sessionToken: access });
        if (status.authenticated && status.user) {
          // The transport's auth gate may have refreshed the token mid-call (an expired token is
          // re-minted on the way out), so report the authoritative stored token, not the local one.
          return { token: storage.getAccess() ?? access, user: status.user, vaultState: status.vaultState };
        }
      }
      if (storage.getRefresh()) {
        const token = await store.ensureFreshAccessToken();
        if (token) {
          const status = await client.getAuthStatus({ sessionToken: token });
          if (status.authenticated && status.user) {
            return { token, user: status.user, vaultState: status.vaultState };
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
          const { token, user, vaultState } = await establishSession();
          if (cancelled) return;
          setState(signedInState(user, token, vaultState));
          // A daemon that restarted since this lineage last refreshed holds none of the operator's
          // credentials open until a refresh presents the unlock key — do that now rather than
          // when the access token next lapses. The refresh reports the reopened vault's state.
          void store.reopenVaultOnLoad();
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
    ({ sessionToken, refreshToken, vaultUnlockKey, vaultState, user }: WholeSession) => {
      storage.set(sessionToken, refreshToken, vaultUnlockKey);
      setState(signedInState(user, sessionToken, vaultState));
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
        const session = checkWholeSession(await client.exchangeCode({ code, state }));
        if ("missing" in session) {
          throw new Error(noWholeSessionMessage("The daemon exchanged the sign-in code", session.missing));
        }
        adoptSession(session.whole);
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
  const deviceAttemptRef = useRef<DeviceAttemptSlot>({ generation: 0, timer: null });

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

    setDeviceLogin({ phase: "starting" });
    let grant;
    try {
      grant = await client.startDeviceLogin({});
    } catch (e) {
      if (isCurrent()) setDeviceLogin(deviceLoginError(e, "Failed to start device sign-in"));
      return;
    }
    if (!isCurrent()) return;

    // GitHub's floor between polls. A slow-down answer raises it for every later poll. An interval
    // that is not positive is a protocol error, never a reason to poll with no delay.
    const grantedIntervalMs = intervalMsOf(grant.intervalSeconds);
    if (grantedIntervalMs === null) {
      setDeviceLogin({
        phase: "failed",
        error: "The daemon issued a device sign-in code with no interval between polls",
      });
      return;
    }

    setDeviceLogin({
      phase: "awaiting-approval",
      userCode: grant.userCode,
      verificationUri: grant.verificationUri,
    });
    const { deviceCode } = grant;
    scheduleDevicePoll({ client, deviceCode, attempt, isCurrent, setDeviceLogin, adoptSession }, grantedIntervalMs);
  }, [client, adoptSession, endDeviceAttempt]);

  const logout = useCallback(async () => {
    await store.logout();
    setState(LOGGED_OUT);
  }, [store]);

  // The credential vault prompt's two actions. A refusal (a wrong passphrase, one too short)
  // rejects, for the prompt to show; success reports the vault open through `onVaultStateChange`.
  const unlockVault = useCallback(
    (passphrase: string, create: boolean) => store.unlockVault(passphrase, create),
    [store],
  );
  const resetVault = useCallback((newPassphrase: string) => store.resetVault(newPassphrase), [store]);

  return { ...state, login, handleCallback, logout, deviceLogin, startDeviceLogin, unlockVault, resetVault };
}
