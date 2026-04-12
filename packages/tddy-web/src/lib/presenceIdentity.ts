/**
 * LiveKit identity for web dashboard presence.
 *
 * Persisted in `sessionStorage` so React remounts (Strict Mode, HMR) reuse the same
 * value. Otherwise `useCommonRoom` would disconnect and rejoin on every remount, which
 * looks like periodic “session refresh” in the participant list.
 *
 * Each browser tab has its own `sessionStorage`, so two tabs still get different
 * identities and avoid evicting each other (`DUPLICATE_IDENTITY`).
 */
const PRESENCE_ID_STORAGE_PREFIX = "tddy.livekit.presenceIdentity:";

export function presenceIdentityForUser(login: string): string {
  const safe = login.trim() || "user";
  if (typeof sessionStorage === "undefined") {
    return `web-${safe}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  }
  const key = `${PRESENCE_ID_STORAGE_PREFIX}${safe}`;
  let id = sessionStorage.getItem(key);
  if (!id) {
    id = `web-${safe}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    sessionStorage.setItem(key, id);
  }
  return id;
}
