/**
 * Pure selection helpers for per-table session bulk select (Connection screen).
 */

/** Header checkbox visual state derived from counts (browser maps indeterminate separately). */
export function computeHeaderCheckboxState(
  selectedCount: number,
  totalRows: number,
): { checked: boolean; indeterminate: boolean } {
  if (totalRows <= 0) {
    return { checked: false, indeterminate: false };
  }
  if (selectedCount <= 0) {
    return { checked: false, indeterminate: false };
  }
  if (selectedCount >= totalRows) {
    return { checked: true, indeterminate: false };
  }
  return { checked: false, indeterminate: true };
}

/** Toggle select-all: all selected → clear; otherwise select every id in the table. */
export function toggleSelectAllForTable(
  allSessionIds: string[],
  selected: ReadonlySet<string>,
): Set<string> {
  const idsSet = new Set(allSessionIds);
  const allSelected =
    allSessionIds.length > 0 &&
    allSessionIds.every((id) => selected.has(id)) &&
    [...selected].every((id) => idsSet.has(id));

  if (allSelected) {
    return new Set();
  }
  return new Set(allSessionIds);
}

/** Toggle one row id in the selection set for this table. */
export function toggleRowInTableSelection(
  selected: ReadonlySet<string>,
  sessionId: string,
): Set<string> {
  const next = new Set(selected);
  if (next.has(sessionId)) {
    next.delete(sessionId);
  } else {
    next.add(sessionId);
  }
  return next;
}
