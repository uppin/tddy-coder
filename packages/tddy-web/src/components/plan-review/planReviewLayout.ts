/**
 * Pure helpers for activity-pane layout validation (terminal + plan side-by-side).
 */
export function terminalAndPlanAreSideBySide(
  terminalRect: { left: number; right: number; top: number; bottom: number },
  planRect: { left: number; right: number; top: number; bottom: number },
  viewportWidth: number,
  viewportHeight: number,
  tolerancePx = 2,
): boolean {
  const t = tolerancePx;

  const termW = terminalRect.right - terminalRect.left;
  const planW = planRect.right - planRect.left;
  const termH = terminalRect.bottom - terminalRect.top;
  const planH = planRect.bottom - planRect.top;
  if (termW <= 0 || planW <= 0 || termH <= 0 || planH <= 0) {
    return false;
  }

  /** Adjacent columns: no horizontal overlap beyond a shared edge (within tolerance). */
  const adjacentColumns =
    terminalRect.right <= planRect.left + t || planRect.right <= terminalRect.left + t;
  if (!adjacentColumns) {
    return false;
  }

  /** Both panes occupy the same vertical band (typical split view). */
  const verticalOverlap =
    Math.min(terminalRect.bottom, planRect.bottom) - Math.max(terminalRect.top, planRect.top);
  if (verticalOverlap <= 0) {
    return false;
  }

  if (
    terminalRect.left < -t ||
    planRect.left < -t ||
    terminalRect.right > viewportWidth + t ||
    planRect.right > viewportWidth + t ||
    terminalRect.top < -t ||
    planRect.top < -t ||
    terminalRect.bottom > viewportHeight + t ||
    planRect.bottom > viewportHeight + t
  ) {
    return false;
  }

  return true;
}
