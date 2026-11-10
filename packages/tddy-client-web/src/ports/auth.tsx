/**
 * The auth port: the signed-in state and its actions, as `useAuth` returned them before the split.
 * The app supplies the implementation (session-token store, OAuth, device flow, vault).
 */
import { createContext, useContext, type ReactElement, type ReactNode } from "react";
import type { GitHubUser, VaultState } from "../gen/auth_pb";

/** Where a device-flow sign-in stands. */
export type DeviceLogin =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "awaiting-approval"; userCode: string; verificationUri: string }
  | { phase: "denied" }
  | { phase: "expired" }
  | { phase: "failed"; error: string };

export interface AuthState {
  user: GitHubUser | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
  sessionToken: string | null;
  isRefreshing: boolean;
  vaultState: VaultState;
}

export interface AuthPort extends AuthState {
  readonly deviceLogin: DeviceLogin;
  login(returnTo?: string): Promise<void>;
  handleCallback(code: string, state: string): Promise<void>;
  logout(): Promise<void>;
  startDeviceLogin(): Promise<void>;
  unlockVault(passphrase: string, create: boolean): Promise<void>;
  resetVault(newPassphrase: string): Promise<void>;
}

const AuthPortContext = createContext<AuthPort | null>(null);

/** Mounts `port` for the subtree. {@link useAuth} throws, naming this component, without it. */
export function AuthPortProvider({ port, children }: { port: AuthPort; children?: ReactNode }): ReactElement {
  return <AuthPortContext.Provider value={port}>{children}</AuthPortContext.Provider>;
}

/** The auth port in scope; throws naming the port when no {@link AuthPortProvider} wraps the caller. */
export function useAuth(): AuthPort {
  const port = useContext(AuthPortContext);
  if (!port) {
    throw new Error("the auth port is not provided: wrap this component in an AuthPortProvider");
  }
  return port;
}
