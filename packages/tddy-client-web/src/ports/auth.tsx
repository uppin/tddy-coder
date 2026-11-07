/**
 * The auth port: the signed-in state and its actions, as `useAuth` returned them before the split.
 * The app supplies the implementation (session-token store, OAuth, device flow, vault).
 */
import type { ReactElement, ReactNode } from "react";
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

export function AuthPortProvider(props: { port: AuthPort; children?: ReactNode }): ReactElement {
  void props;
  throw new Error("TODO(clients): implement");
}

/** The auth port in scope; throws naming the port when no {@link AuthPortProvider} wraps the caller. */
export function useAuth(): AuthPort {
  throw new Error("TODO(clients): implement");
}
