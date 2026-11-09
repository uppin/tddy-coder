/**
 * A navigation port over the real `window.location.hash`, for the specs whose assertions are about
 * the URL the operator sees (deep links, Back, an inbound URL change).
 *
 * The app's own binding lives in `tddy-web` and a package never imports it, so this is the test-side
 * counterpart: the same grammar (`parseAppLocation` / `formatAppLocation`), the same push-versus-replace
 * rule, and `hashchange` / `popstate` as the change signal. `appLocationPage` reads and writes the same
 * hash, so a spec states the URL and checks it without naming the port.
 */
import { formatAppLocation, parseAppLocation, type AppLocation, type NavigationPort } from "tddy-client-web";

const listeners = new Set<() => void>();
let cachedHash: string | null = null;
let cachedLocation: AppLocation = { path: "/", params: {} };

function read(): AppLocation {
  const hash = window.location.hash;
  if (hash !== cachedHash) {
    cachedHash = hash;
    cachedLocation = parseAppLocation(hash);
  }
  return cachedLocation;
}

function notify(): void {
  for (const listener of [...listeners]) listener();
}

export const hashNavigationPort: NavigationPort = {
  location: read,
  navigate: (to, options) => {
    const hash = formatAppLocation(typeof to === "string" ? parseAppLocation(to) : to);
    if (options?.replace) window.history.replaceState(null, "", hash);
    else window.location.hash = hash.slice(1);
    notify();
  },
  subscribe: (listener) => {
    listeners.add(listener);
    if (listeners.size === 1) {
      window.addEventListener("hashchange", notify);
      window.addEventListener("popstate", notify);
    }
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        window.removeEventListener("hashchange", notify);
        window.removeEventListener("popstate", notify);
      }
    };
  },
};
