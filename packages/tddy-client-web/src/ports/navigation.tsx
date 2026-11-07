/**
 * The navigation port: the app location (hash path + params) a component reads and changes. The app
 * supplies the implementation (`tddy-web` binds it to `window.location.hash`); specs supply a double.
 */
import {
  createContext,
  useCallback,
  useContext,
  useSyncExternalStore,
  type ReactElement,
  type ReactNode,
} from "react";
import { withParams, withPath, type AppLocation } from "../appLocation";

export type { AppLocation };

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

const NavigationContext = createContext<NavigationPort | null>(null);

/** Mounts `port` for the subtree. {@link useAppLocation} throws, naming this component, without it. */
export function NavigationProvider({ port, children }: { port: NavigationPort; children?: ReactNode }): ReactElement {
  return <NavigationContext.Provider value={port}>{children}</NavigationContext.Provider>;
}

/** The navigation port in scope; throws naming the port when no {@link NavigationProvider} wraps the caller. */
export function useNavigationPort(): NavigationPort {
  const port = useContext(NavigationContext);
  if (!port) {
    throw new Error("the navigation port is not provided: wrap this component in a NavigationProvider");
  }
  return port;
}

/** The location the port reports, re-read whenever the port says it changed. */
export function useAppLocation(): UseAppLocationResult {
  const port = useNavigationPort();
  const subscribe = useCallback((listener: () => void) => port.subscribe(listener), [port]);
  const snapshot = useCallback(() => port.location(), [port]);
  const location = useSyncExternalStore(subscribe, snapshot, snapshot);
  const navigate = useCallback(
    (path: string, options?: NavigateOptions) => port.navigate(withPath(port.location(), path), options),
    [port],
  );
  const setParams = useCallback(
    (patch: Readonly<Record<string, string | null>>, options?: NavigateOptions) =>
      port.navigate(withParams(port.location(), patch), options),
    [port],
  );
  return { location, navigate, setParams };
}
