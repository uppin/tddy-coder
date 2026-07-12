/**
 * Window-bound "refresh the session list" bridge — a non-React signal any screen can raise so the
 * sessions drawer re-fetches the selected host's `ListSessions`, without threading a `refresh`
 * callback through React props/context.
 *
 * Mirrors `lib/terminalZoomBridge.ts`: a `CustomEvent` on `window`. The sessions drawer subscribes
 * its fetch to it (see `SessionsDrawerScreen`); action handlers anywhere (Terminate, session
 * created, a background poller, another screen) call `requestSessionsRefresh()`.
 *
 * Note: active cross-host sessions are already live via LiveKit participant presence and need no
 * refresh — this only re-pulls the selected host's on-disk session rows (labels, status, inactive
 * history).
 */

export const SESSIONS_REFRESH_EVENT = "tddy-sessions-refresh";

/** Dispatch on a specific `Window` (Cypress component tests run in the AUT iframe's `window`). */
export function requestSessionsRefreshOn(target: Window): void {
  target.dispatchEvent(new CustomEvent(SESSIONS_REFRESH_EVENT, { bubbles: false }));
}

/** Request a session-list refresh. No-op outside a browser. */
export function requestSessionsRefresh(): void {
  if (typeof window === "undefined") return;
  requestSessionsRefreshOn(window);
}

/**
 * Subscribe `handler` to session-refresh requests; returns an unsubscribe function. No-op (returns a
 * no-op unsubscribe) outside a browser.
 */
export function subscribeSessionsRefresh(handler: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const listener = () => handler();
  window.addEventListener(SESSIONS_REFRESH_EVENT, listener);
  return () => window.removeEventListener(SESSIONS_REFRESH_EVENT, listener);
}
