import { useState, useEffect, useMemo, useCallback } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { AuthService } from "../gen/auth_pb";
import type { GitHubUser } from "../gen/auth_pb";

const SESSION_TOKEN_KEY = "tddy_session_token";
const OAUTH_STATE_KEY = "tddy_oauth_state";

function createAuthClient() {
  const transport = createConnectTransport({
    baseUrl:
      typeof window !== "undefined"
        ? `${window.location.origin}/rpc`
        : "",
    useBinaryFormat: true,
  });
  return createClient(AuthService, transport);
}

export interface AuthState {
  user: GitHubUser | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
  /** Session token for RPC calls (when authenticated). */
  sessionToken: string | null;
}

export function useAuth() {
  const client = useMemo(() => createAuthClient(), []);
  const [state, setState] = useState<AuthState>({
    user: null,
    isAuthenticated: false,
    isLoading: true,
    error: null,
    sessionToken: null,
  });

  // On mount: check stored session
  useEffect(() => {
    const token = localStorage.getItem(SESSION_TOKEN_KEY);
    if (!token) {
      setState((s) => ({ ...s, isLoading: false }));
      return;
    }
    client
      .getAuthStatus({ sessionToken: token })
      .then((res) => {
        if (res.authenticated && res.user) {
          setState({
            user: res.user,
            isAuthenticated: true,
            isLoading: false,
            error: null,
            sessionToken: token,
          });
        } else {
          localStorage.removeItem(SESSION_TOKEN_KEY);
          setState({ user: null, isAuthenticated: false, isLoading: false, error: null, sessionToken: null });
        }
      })
      .catch(() => {
        localStorage.removeItem(SESSION_TOKEN_KEY);
        setState({ user: null, isAuthenticated: false, isLoading: false, error: null, sessionToken: null });
      });
  }, [client]);

  const login = useCallback(async () => {
    try {
      const res = await client.getAuthUrl({});
      sessionStorage.setItem(OAUTH_STATE_KEY, res.state);
      window.location.href = res.authorizeUrl;
    } catch (e) {
      setState((s) => ({
        ...s,
        error: e instanceof Error ? e.message : "Failed to get auth URL",
      }));
    }
  }, [client]);

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
        localStorage.setItem(SESSION_TOKEN_KEY, res.sessionToken);
        setState({
          user: res.user ?? null,
          isAuthenticated: true,
          isLoading: false,
          error: null,
          sessionToken: res.sessionToken,
        });
      } catch (e) {
        setState({
          user: null,
          isAuthenticated: false,
          isLoading: false,
          error: e instanceof Error ? e.message : "Code exchange failed",
          sessionToken: null,
        });
      }
    },
    [client],
  );

  const logout = useCallback(async () => {
    const token = localStorage.getItem(SESSION_TOKEN_KEY);
    if (token) {
      try {
        await client.logout({ sessionToken: token });
      } catch {
        // Ignore logout errors
      }
    }
    localStorage.removeItem(SESSION_TOKEN_KEY);
    setState({ user: null, isAuthenticated: false, isLoading: false, error: null, sessionToken: null });
  }, [client]);

  return { ...state, login, handleCallback, logout };
}
