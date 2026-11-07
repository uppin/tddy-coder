/**
 * The navigation port: the app location (hash path + params) a component reads and changes. The app
 * supplies the implementation (`tddy-web` binds it to `window.location.hash`); specs supply a double.
 */
import type { ReactElement, ReactNode } from "react";

export interface AppLocation {
  /** The hash path, always leading-slashed (`/sessions/abc`). */
  readonly path: string;
  /** The hash-local query params, decoded. */
  readonly params: Readonly<Record<string, string>>;
}

export interface NavigateOptions {
  replace?: boolean;
}

export interface NavigationPort {
  location(): AppLocation;
  navigate(to: string | AppLocation, options?: NavigateOptions): void;
  subscribe(listener: () => void): () => void;
}

export interface UseAppLocationResult {
  readonly location: AppLocation;
  readonly navigate: (path: string, options?: NavigateOptions) => void;
  readonly setParams: (patch: Readonly<Record<string, string | null>>, options?: NavigateOptions) => void;
}

export function NavigationProvider(props: { port: NavigationPort; children?: ReactNode }): ReactElement {
  void props;
  throw new Error("TODO(clients): implement");
}

export function useAppLocation(): UseAppLocationResult {
  throw new Error("TODO(clients): implement");
}
