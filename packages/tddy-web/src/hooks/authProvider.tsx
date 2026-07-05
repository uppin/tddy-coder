import { createContext, useContext, type ReactNode } from "react";
import { useAuth } from "./useAuth";

type AuthContextValue = ReturnType<typeof useAuth>;

const AuthContext = createContext<AuthContextValue | null>(null);

/**
 * Owns the *only* `useAuth()` instance for the app — its access + refresh token pair, the single
 * session-token store that mints fresh access tokens on demand, and its login/logout actions — and
 * provides it to every descendant via context.
 *
 * Mount once near the app root (inside `RpcTransportProvider`, above everything else). Unlike
 * `useHttpTransport`/`useSelectedDaemon` (which fall back to inert per-instance defaults when
 * unwrapped), `useAuthContext` throws without a provider: `useAuth()` is stateful — it resolves the
 * session and installs the transport's auth-token gate — so silently falling back to an independent
 * instance per consumer would recreate exactly the bug this provider exists to fix (every screen
 * mounting its own uncoordinated `useAuth()`, each with its own store and gate registration). A
 * required provider is the correct contract here.
 */
export function AuthProvider({ children }: { children: ReactNode }) {
  const auth = useAuth();
  return <AuthContext.Provider value={auth}>{children}</AuthContext.Provider>;
}

/** Read the shared session/auth state. Throws when rendered outside an `AuthProvider`. */
export function useAuthContext(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuthContext must be used within an AuthProvider");
  }
  return ctx;
}
