import type { TerminalEngine } from "../engine/TerminalEngine";

/** CSS width of the rendered grid (`.xterm-screen`, not the full-width xterm root). Used for X-axis font fit. */
export function measureTerminalScreenCssWidth(term: TerminalEngine): number {
  return term.screenElement?.getBoundingClientRect().width ?? 0;
}

/** CSS height of the rendered grid. Used to convert drag pixels into scroll lines. */
export function measureTerminalScreenCssHeight(term: TerminalEngine): number {
  return term.screenElement?.getBoundingClientRect().height ?? 0;
}

/** Resizes the engine to the grid that fits its container, when it can measure one and it differs. */
export function fitTerminalToContainer(term: TerminalEngine): void {
  const proposed = term.proposeDimensions();
  if (!proposed) return;
  if (proposed.cols !== term.cols || proposed.rows !== term.rows) {
    term.resize(proposed.cols, proposed.rows);
  }
}

/** Limit Ctrl/Cmd +/-/0 so we do not steal shortcuts from other page inputs (e.g. browser address bar). */
export function shouldHandleTerminalZoomKeys(): boolean {
  const el = document.activeElement;
  if (!el || el === document.body) return true;
  return el.closest("[data-testid='terminal']") !== null;
}
