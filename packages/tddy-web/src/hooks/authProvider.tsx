import { createContext, useContext, type ReactNode } from "react";
import { useAuth } from "./useAuth";

type AuthContextValue = ReturnType<typeof useAuth>;

const AuthContext = createContext<AuthContextValue | null>(null);

/**
 * Owns the *only* `useAuth()` instance for the app — its session token, its single 4-minute
 * refresh timer, and its login/logout actions — and provides it to every descendant via context.
 *
 * Mount once near the app root (inside `RpcTransportProvider`, above everything else). Unlike
 * `useHttpTransport`/`useSelectedDaemon` (which fall back to inert per-instance defaults when
 * unwrapped), `useAuthContext` throws without a provider: `useAuth()` is stateful — it fetches
 * auth status and starts a live refresh timer — so silently falling back to an independent
 * instance per consumer would recreate exactly the bug this provider exists to fix (every screen
 * mounting its own uncoordinated `useAuth()`, some of which get destroyed on a daemon-switch
 * remount before their next refresh fires). A required provider is the correct contract here.
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
