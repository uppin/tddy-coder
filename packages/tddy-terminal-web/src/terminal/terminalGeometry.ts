import type { Terminal } from "ghostty-web";

/** Canvas/CSS width of the rendered grid (not the full-width xterm root). Used for X-axis font fit. */
export function measureTerminalCanvasCssWidth(term: Terminal): number {
  const root = term.element;
  if (!root) return 0;
  const canvas = root.querySelector("canvas");
  if (!canvas) return 0;
  return canvas.getBoundingClientRect().width;
}

/** Canvas/CSS height of the rendered grid. Used to convert drag pixels into scroll lines. */
export function measureTerminalCanvasCssHeight(term: Terminal): number {
  const root = term.element;
  if (!root) return 0;
  const canvas = root.querySelector("canvas");
  if (!canvas) return 0;
  return canvas.getBoundingClientRect().height;
}

/** Limit Ctrl/Cmd +/-/0 so we do not steal shortcuts from other page inputs (e.g. browser address bar). */
export function shouldHandleTerminalZoomKeys(): boolean {
  const el = document.activeElement;
  if (!el || el === document.body) return true;
  return el.closest("[data-testid='ghostty-terminal']") !== null;
}
