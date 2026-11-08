import { useCallback, useEffect, useState } from "react";
import type { FitAddon } from "ghostty-web";
import {
  canPitchIn,
  canPitchOut,
  clampTerminalFontSize,
  pitchInFontSize,
  pitchOutFontSize,
} from "../lib/terminalZoom";
import {
  dispatchTerminalFontSizeSync,
  dispatchTerminalZoomBridge,
  parseTerminalZoomBridgeDetail,
  TERMINAL_ZOOM_BRIDGE_EVENT,
} from "../lib/terminalZoomBridge";
import { TERMINAL_OVERLAY_FIXED_GRID_CHAR_WIDTH_EM } from "../connection/config";
import { measureTerminalCanvasCssWidth, shouldHandleTerminalZoomKeys } from "./terminalGeometry";
import type { ContainerRef, TerminalRef } from "./terminalRefs";

export interface TerminalFontSizeInput {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  fitAddonRef: { readonly current: FitAddon | null };
  ready: boolean;
  sessionActive: boolean;
  fontSize: number;
  minFontSize: number;
  maxFontSize: number;
  fixedViewportGrid: { cols: number; rows: number } | undefined;
  zoomVerbose: boolean;
}

/**
 * Font size, fit and zoom: applies a size and refits, keeps a fixed grid sized to its pane, and
 * answers the zoom bridge event and the Ctrl/Cmd +/-/0 shortcuts.
 */
export function useTerminalFontSize({
  containerRef,
  termRef,
  fitAddonRef,
  ready,
  sessionActive,
  fontSize,
  minFontSize,
  maxFontSize,
  fixedViewportGrid,
  zoomVerbose,
}: TerminalFontSizeInput): {
  applyFontSizePx: (px: number, bounds?: { min: number; max: number }) => void;
  displayFontSize: number;
} {
  const [displayFontSize, setDisplayFontSize] = useState(fontSize);

  const applyFontSizePx = useCallback(
    (px: number, bounds?: { min: number; max: number }) => {
      if (!Number.isFinite(px)) return;
      const min = bounds?.min ?? minFontSize;
      const max = bounds?.max ?? maxFontSize;
      const clamped = clampTerminalFontSize(px, min, max);
      const term = termRef.current;
      const fit = fitAddonRef.current;
      if (zoomVerbose) {
        console.info("[tddy][GhosttyTerminal] applyFontSizePx", {
          requested: px,
          clamped,
          hasTerm: !!term,
          colsBefore: term?.cols,
          rowsBefore: term?.rows,
        });
      }
      if (term) {
        term.options.fontSize = clamped;
        if (fixedViewportGrid) {
          term.resize(fixedViewportGrid.cols, fixedViewportGrid.rows);
        } else {
          fit?.fit();
        }
        if (zoomVerbose) {
          console.debug("[tddy][GhosttyTerminal] after font/geometry", {
            cols: term.cols,
            rows: term.rows,
            fontSize: term.options.fontSize,
            fixedGrid: !!fixedViewportGrid,
          });
        }
      }
      setDisplayFontSize(clamped);
      dispatchTerminalFontSizeSync(clamped);
    },
    [minFontSize, maxFontSize, zoomVerbose, fixedViewportGrid]
  );

  useEffect(() => {
    if (!ready || !termRef.current || fixedViewportGrid) return;
    applyFontSizePx(fontSize, { min: minFontSize, max: maxFontSize });
  }, [ready, fontSize, minFontSize, maxFontSize, applyFontSizePx, fixedViewportGrid]);

  useEffect(() => {
    if (!ready || !fixedViewportGrid || !containerRef.current || !termRef.current) return;
    const term = termRef.current;
    const el = containerRef.current;
    const { cols: gc, rows: gr } = fixedViewportGrid;

    const sync = () => {
      const w = el.clientWidth;
      const h = el.clientHeight;
      if (w < 4 || h < 4) return;

      /**
       * `term.element` is typically width:100% of the pane, so its offsetWidth does not change with
       * fontSize — binary search on that always picked maxFontSize and ignored resize. Size from pane
       * width (X) and refine against the canvas grid width.
       */
      const em = TERMINAL_OVERLAY_FIXED_GRID_CHAR_WIDTH_EM;
      let chosen = clampTerminalFontSize(w / (gc * em), minFontSize, maxFontSize);
      term.options.fontSize = chosen;
      term.resize(gc, gr);

      for (let i = 0; i < 8; i++) {
        void term.element?.offsetWidth;
        const innerW = measureTerminalCanvasCssWidth(term);
        if (innerW <= 0) break;
        if (Math.abs(innerW - w) <= 1.5) break;
        chosen = clampTerminalFontSize(chosen * (w / innerW), minFontSize, maxFontSize);
        term.options.fontSize = chosen;
        term.resize(gc, gr);
      }

      setDisplayFontSize(chosen);
      dispatchTerminalFontSizeSync(chosen);
    };

    const ro = new ResizeObserver(() => sync());
    ro.observe(el);
    sync();
    return () => ro.disconnect();
  }, [ready, fixedViewportGrid?.cols, fixedViewportGrid?.rows, minFontSize, maxFontSize]);

  useEffect(() => {
    if (fixedViewportGrid) return;
    const onBridge = (ev: Event) => {
      const ce = ev as CustomEvent<unknown>;
      const parsed = parseTerminalZoomBridgeDetail(ce.detail);
      if (!parsed) return;
      const d = parsed;
      const merged = {
        min: d.opts?.min ?? minFontSize,
        max: d.opts?.max ?? maxFontSize,
      };
      const term = termRef.current;
      if (zoomVerbose) {
        console.debug("[tddy][GhosttyTerminal] terminal zoom bridge", {
          action: d.action,
          merged,
          hasTerm: !!term,
        });
      }
      if (!term) {
        if (zoomVerbose) {
          console.info("[tddy][GhosttyTerminal] zoom bridge ignored — terminal not ready");
        }
        return;
      }
      const current = term.options.fontSize;
      if (d.action === "reset") {
        applyFontSizePx(d.baselineFontSize, merged);
        return;
      }
      if (d.action === "pitch-in") {
        applyFontSizePx(pitchInFontSize(current, merged), merged);
        return;
      }
      if (d.action === "pitch-out") {
        applyFontSizePx(pitchOutFontSize(current, merged), merged);
      }
    };
    window.addEventListener(TERMINAL_ZOOM_BRIDGE_EVENT, onBridge as EventListener);
    return () =>
      window.removeEventListener(TERMINAL_ZOOM_BRIDGE_EVENT, onBridge as EventListener);
  }, [minFontSize, maxFontSize, applyFontSizePx, zoomVerbose, fixedViewportGrid]);

  useEffect(() => {
    if (!sessionActive || fixedViewportGrid) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      if (!shouldHandleTerminalZoomKeys()) return;
      const term = termRef.current;
      if (!term) return;
      const live = term.options.fontSize;
      const stepOpts = { min: minFontSize, max: maxFontSize };
      const key = e.key;
      if (key === "+" || key === "=") {
        if (!canPitchIn(live, stepOpts)) return;
        e.preventDefault();
        dispatchTerminalZoomBridge({
          action: "pitch-in",
          baselineFontSize: fontSize,
          opts: stepOpts,
        });
        return;
      }
      if (key === "-" || key === "_") {
        if (!canPitchOut(live, stepOpts)) return;
        e.preventDefault();
        dispatchTerminalZoomBridge({
          action: "pitch-out",
          baselineFontSize: fontSize,
          opts: stepOpts,
        });
        return;
      }
      if (key === "0") {
        e.preventDefault();
        dispatchTerminalZoomBridge({
          action: "reset",
          baselineFontSize: fontSize,
          opts: stepOpts,
        });
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [sessionActive, minFontSize, maxFontSize, fontSize, fixedViewportGrid]);

  return { applyFontSizePx, displayFontSize };
}
