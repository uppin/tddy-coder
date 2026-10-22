/**
 * Per-viewer memory of which project groups the drawer shows collapsed (`#project-home` 2/7).
 *
 * Browser storage, wrapped: storage that cannot be read means "every group expanded", and a write
 * that cannot be made is dropped — the drawer never depends on it.
 */

/** Project ids (or `"other"`) the viewer collapsed. */
export function readCollapsedProjects(): Set<string> {
  // TODO(project-groups): implement
  return new Set();
}

/** Remember `ids` as the collapsed set. */
export function writeCollapsedProjects(ids: ReadonlySet<string>): void {
  // TODO(project-groups): implement
  void ids;
}
