/**
 * The browser side of the URL grammar: one module-level store over `window.location.hash`, read
 * through `useSyncExternalStore`.
 *
 * Module-level rather than a React context so a nested screen — and a Cypress component test that
 * mounts one screen directly, with no router above it — shares the same source of truth without
 * prop-drilling a navigate callback through every pane.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 */

import { useCallback, useSyncExternalStore } from "react";
import {
  formatAppLocation,
  parseAppLocation,
  withParams,
  withPath,
  type AppLocation,
} from "./appLocation";

const listeners = new Set<() => void>();

// `useSyncExternalStore` compares snapshots by identity and re-renders on any change, so the parsed
// location must be cached against the hash it came from — re-parsing per call would return a fresh
// object every time and loop.
let cachedHash: string | null = null;
let cachedLocation: AppLocation = { path: "/", params: {} };

function currentHash(): string {
  return typeof window === "undefined" ? "" : window.location.hash;
}

/** The current location, re-parsed only when the hash actually changed. */
export function readAppLocation(): AppLocation {
  const hash = currentHash();
  if (hash !== cachedHash) {
    cachedHash = hash;
    cachedLocation = parseAppLocation(hash);
  }
  return cachedLocation;
}

function notify(): void {
  for (const listener of [...listeners]) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (listeners.size === 1 && typeof window !== "undefined") {
    window.addEventListener("hashchange", notify);
    window.addEventListener("popstate", notify);
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0 && typeof window !== "undefined") {
      window.removeEventListener("hashchange", notify);
      window.removeEventListener("popstate", notify);
    }
  };
}

export interface NavigateOptions {
  /**
   * Rewrite the current history entry instead of adding one. Use for changes the app made on the
   * operator's behalf (canonicalisation, an auto-opened inspector, a resolved default written back):
   * Back should not step through states nobody chose.
   */
  replace?: boolean;
}

/**
 * Move to `to` — a hash path (`"/sessions/abc"`, optionally with its own `?query`) or a built
 * {@link AppLocation}.
 *
 * A push assigns `location.hash`, which the browser records as a history entry; `hashchange` then
 * fires asynchronously, so subscribers are notified synchronously here as well. The later event is
 * a no-op because the cached snapshot already matches.
 */
export function navigateAppLocation(to: string | AppLocation, options?: NavigateOptions): void {
  if (typeof window === "undefined") return;
  const hash = formatAppLocation(typeof to === "string" ? parseAppLocation(to) : to);
  if (options?.replace) {
    window.history.replaceState(null, "", hash);
  } else {
    window.location.hash = hash.slice(1);
  }
  notify();
}

/** Apply a param patch to the current location and navigate to the result. */
export function setAppLocationParams(
  patch: Readonly<Record<string, string | null>>,
  options?: NavigateOptions,
): void {
  navigateAppLocation(withParams(readAppLocation(), patch), options);
}

/** Move to `path`, letting {@link withPath} decide which params come along. */
function setAppLocationPath(path: string, options?: NavigateOptions): void {
  navigateAppLocation(withPath(readAppLocation(), path), options);
}

export interface UseAppLocationResult {
  readonly location: AppLocation;
  /** Navigate to a path, carrying params per {@link withPath}. */
  readonly navigate: (path: string, options?: NavigateOptions) => void;
  /** Patch params on the current path. A `null` value deletes the param. */
  readonly setParams: (
    patch: Readonly<Record<string, string | null>>,
    options?: NavigateOptions,
  ) => void;
}

/** Subscribe to the app location and get the navigation helpers bound to it. */
export function useAppLocation(): UseAppLocationResult {
  const location = useSyncExternalStore(subscribe, readAppLocation, readAppLocation);
  const navigate = useCallback(
    (path: string, options?: NavigateOptions) => setAppLocationPath(path, options),
    [],
  );
  const setParams = useCallback(
    (patch: Readonly<Record<string, string | null>>, options?: NavigateOptions) =>
      setAppLocationParams(patch, options),
    [],
  );
  return { location, navigate, setParams };
}
