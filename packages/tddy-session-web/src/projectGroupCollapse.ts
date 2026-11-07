/**
 * Per-viewer memory of which project groups the drawer shows collapsed (`#project-home` 2/7).
 *
 * Browser storage, wrapped: storage that cannot be read means "every group expanded", and a write
 * that cannot be made is dropped — the drawer never depends on it.
 */

const STORAGE_KEY = "tddy.sessionsDrawer.collapsedProjects";

/** Project ids (or `"other"`) the viewer collapsed. */
export function readCollapsedProjects(): Set<string> {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return new Set();
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id): id is string => typeof id === "string"));
  } catch {
    return new Set();
  }
}

/** Remember `ids` as the collapsed set. */
export function writeCollapsedProjects(ids: ReadonlySet<string>): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify([...ids]));
  } catch {
    // Unwritable storage: the collapse still applies for this mount, it just is not remembered.
  }
}
