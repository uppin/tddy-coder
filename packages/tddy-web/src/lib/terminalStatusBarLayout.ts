/** Axis-aligned rectangle in viewport coordinates (e.g. from getBoundingClientRect). */
export interface ViewRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

function pointInRect(px: number, py: number, r: ViewRect): boolean {
  return px >= r.left && px <= r.right && py >= r.top && py <= r.bottom;
}

/**
 * True when the center point of `inner` lies inside `outer` using **inclusive** rectangle edges
 * (`>=` / `<=`). The name “strictly” refers to the center point being strictly inside the outer
 * area as a region, not strict inequalities on coordinates.
 */
export function controlCenterStrictlyInsideRect(inner: ViewRect, outer: ViewRect): boolean {
  const cx = (inner.left + inner.right) / 2;
  const cy = (inner.top + inner.bottom) / 2;
  return pointInRect(cx, cy, outer);
}

/**
 * True when the status bar sits above the terminal: statusBar.bottom <= terminal.top + epsilon.
 */
export function statusBarBottomMeetsOrAboveTerminalTop(
  statusBar: ViewRect,
  terminal: ViewRect,
  epsilonPx = 0.5,
): boolean {
  return statusBar.bottom <= terminal.top + epsilonPx;
}

/**
 * True when no control bounding-box center lies inside the terminal canvas rectangle.
 */
export function plannedChromeCentersClearTerminalCanvas(
  terminal: ViewRect,
  controlRects: ViewRect[],
): boolean {
  for (const c of controlRects) {
    const cx = (c.left + c.right) / 2;
    const cy = (c.top + c.bottom) / 2;
    if (pointInRect(cx, cy, terminal)) {
      return false;
    }
  }
  return true;
}
