/**
 * Stable per-browser-tab screen identity for terminal control.
 *
 * Persisted in `sessionStorage` so React remounts (Strict Mode, HMR) reuse the same
 * value. Each browser tab has its own `sessionStorage`, so two tabs get different ids
 * and can be distinguished as separate "screens" by the daemon.
 */
const SCREEN_ID_KEY = "tddy.screenId";

function generateScreenId(): string {
  return `screen-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function getScreenId(): string {
  if (typeof sessionStorage === "undefined") {
    return generateScreenId();
  }
  let id = sessionStorage.getItem(SCREEN_ID_KEY);
  if (!id) {
    id = generateScreenId();
    sessionStorage.setItem(SCREEN_ID_KEY, id);
  }
  return id;
}
