import type { TerminalZoomStepOptions } from "./terminalZoom";

export type TerminalZoomBridgeAction = "pitch-in" | "pitch-out" | "reset";

export interface TerminalZoomBridgeDetail {
  action: TerminalZoomBridgeAction;
  /** Session baseline font (for reset). */
  baselineFontSize: number;
  opts?: TerminalZoomStepOptions;
}

export const TERMINAL_ZOOM_BRIDGE_EVENT = "tddy-terminal-zoom";

/** Emitted when the terminal’s applied font size changes (listeners may sync UI state). */
export const TERMINAL_FONT_SIZE_SYNC_EVENT = "tddy-terminal-font-size-sync";

export interface TerminalFontSizeSyncDetail {
  fontSize: number;
}

/** Opt-in verbose logging for zoom bridge / sync (set `VITE_TERMINAL_ZOOM_DEBUG=true`). */
export function isTerminalZoomDebugEnabled(): boolean {
  try {
    return import.meta.env.VITE_TERMINAL_ZOOM_DEBUG === "true";
  } catch {
    return false;
  }
}

function finiteNumber(n: unknown): n is number {
  return typeof n === "number" && Number.isFinite(n);
}

function parseStepOptions(raw: unknown): TerminalZoomStepOptions | undefined {
  if (raw === undefined) return undefined;
  if (raw === null || typeof raw !== "object") return undefined;
  const o = raw as Record<string, unknown>;
  const min = o.min;
  const max = o.max;
  const step = o.step;
  if (min !== undefined && !finiteNumber(min)) return undefined;
  if (max !== undefined && !finiteNumber(max)) return undefined;
  if (step !== undefined && !finiteNumber(step)) return undefined;
  return {
    min: min as number | undefined,
    max: max as number | undefined,
    step: step as number | undefined,
  };
}

/**
 * Validate bridge event detail from an untrusted CustomEvent.
 * Returns null if the payload is not a well-formed zoom bridge message.
 */
export function parseTerminalZoomBridgeDetail(raw: unknown): TerminalZoomBridgeDetail | null {
  if (raw === null || typeof raw !== "object") return null;
  const d = raw as Record<string, unknown>;
  const action = d.action;
  if (action !== "pitch-in" && action !== "pitch-out" && action !== "reset") return null;
  if (!finiteNumber(d.baselineFontSize) || d.baselineFontSize <= 0) return null;
  let opts: TerminalZoomStepOptions | undefined;
  if (d.opts !== undefined) {
    if (d.opts === null) return null;
    const parsed = parseStepOptions(d.opts);
    if (parsed === undefined) return null;
    opts = parsed;
  }
  return {
    action,
    baselineFontSize: d.baselineFontSize,
    opts,
  };
}

/** Parse font size from a font-sync CustomEvent detail. */
export function parseTerminalFontSizeSyncDetail(raw: unknown): number | null {
  if (raw === null || typeof raw !== "object") return null;
  const d = raw as Record<string, unknown>;
  const fs = d.fontSize;
  if (!finiteNumber(fs) || fs <= 0) return null;
  return fs;
}

/** Dispatch on a specific `Window` (e.g. Cypress component tests use the AUT iframe’s `window`). */
export function dispatchTerminalZoomBridgeOn(
  target: Window,
  detail: TerminalZoomBridgeDetail
): void {
  if (isTerminalZoomDebugEnabled()) {
    console.info("[tddy][terminalZoomBridge] dispatch", {
      action: detail.action,
      baselineFontSize: detail.baselineFontSize,
      opts: detail.opts,
    });
  }
  target.dispatchEvent(
    new CustomEvent<TerminalZoomBridgeDetail>(TERMINAL_ZOOM_BRIDGE_EVENT, {
      detail,
      bubbles: false,
    })
  );
}

export function dispatchTerminalZoomBridge(detail: TerminalZoomBridgeDetail): void {
  if (typeof window === "undefined") return;
  dispatchTerminalZoomBridgeOn(window, detail);
}

export function dispatchTerminalFontSizeSync(fontSize: number): void {
  if (isTerminalZoomDebugEnabled()) {
    console.debug("[tddy][terminalZoomBridge] font size sync", { fontSize });
  }
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<TerminalFontSizeSyncDetail>(TERMINAL_FONT_SIZE_SYNC_EVENT, {
      detail: { fontSize },
      bubbles: false,
    })
  );
}
