import { useEffect, type MutableRefObject } from "react";
import {
  canPitchIn,
  canPitchOut,
  pitchInFontSize,
  pitchOutFontSize,
  reduceTrackpadPinchAccum,
  TRACKPAD_PINCH_STEP_ACCUM_PX,
} from "../lib/terminalZoom";
import type { ContainerRef, TerminalRef } from "./terminalRefs";

/** Finger separation change (px) required before applying one font step during touch pinch. */
const PINCH_FONT_STEP_SPAN_PX = 22;

export interface PinchZoomInput {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  /** Accumulates `deltaY` for trackpad pinch (Ctrl+wheel); touch pinch uses separate logic. */
  wheelPinchAccumRef: MutableRefObject<number>;
  applyFontSizePx: (px: number, bounds?: { min: number; max: number }) => void;
  ready: boolean;
  pinchZoomFont: boolean;
  sessionActive: boolean;
  fixedViewportGrid: { cols: number; rows: number } | undefined;
  minFontSize: number;
  maxFontSize: number;
}

/** Two-finger touch pinch and trackpad (Ctrl+wheel) pinch adjust the font size. */
export function useTerminalPinchZoom({
  containerRef,
  termRef,
  wheelPinchAccumRef,
  applyFontSizePx,
  ready,
  pinchZoomFont,
  sessionActive,
  fixedViewportGrid,
  minFontSize,
  maxFontSize,
}: PinchZoomInput): void {
  // Two-finger pinch → font pitch in/out (mobile); uses finger span like browser zoom.
  useEffect(() => {
    if (!pinchZoomFont || !sessionActive || fixedViewportGrid) return;
    const container = containerRef.current;
    // `ready` is set only after `termRef` is assigned; do not require `termRef` here so
    // listeners attach in the same commit (avoids races with tests / strict mode).
    if (!container || !ready) return;

    let anchorSpan = 0;

    const spanOf = (e: TouchEvent): number => {
      if (e.touches.length < 2) return 0;
      const a = e.touches[0];
      const b = e.touches[1];
      return Math.hypot(b.clientX - a.clientX, b.clientY - a.clientY);
    };

    const stepOpts = () => ({ min: minFontSize, max: maxFontSize });

    const onTouchStart = (e: TouchEvent) => {
      if (e.touches.length === 2) {
        anchorSpan = spanOf(e);
      }
    };

    const onTouchMove = (e: TouchEvent) => {
      if (e.touches.length < 2 || anchorSpan <= 0) return;
      const span = spanOf(e);
      const delta = span - anchorSpan;
      const term = termRef.current;
      if (!term) return;
      const current = term.fontSize();
      const opts = stepOpts();
      if (delta > PINCH_FONT_STEP_SPAN_PX && canPitchIn(current, opts)) {
        applyFontSizePx(pitchInFontSize(current, opts), opts);
        anchorSpan = span;
        e.preventDefault();
      } else if (delta < -PINCH_FONT_STEP_SPAN_PX && canPitchOut(current, opts)) {
        applyFontSizePx(pitchOutFontSize(current, opts), opts);
        anchorSpan = span;
        e.preventDefault();
      }
    };

    const endPinch = (e: TouchEvent) => {
      if (e.touches.length < 2) {
        anchorSpan = 0;
      }
    };

    // Trackpad pinch (macOS / Windows): `wheel` + `ctrlKey` — must not depend on `onData`
    // (the mouse/SGR effect is skipped when `onData` is unset).
    const onWheelTrackpad = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      const t = termRef.current;
      if (!t) return;
      const opts = { min: minFontSize, max: maxFontSize };
      const cur = t.fontSize();
      const { accum, fontSize } = reduceTrackpadPinchAccum(
        wheelPinchAccumRef.current,
        e.deltaY,
        true,
        TRACKPAD_PINCH_STEP_ACCUM_PX,
        cur,
        opts
      );
      wheelPinchAccumRef.current = accum;
      if (fontSize !== cur) {
        applyFontSizePx(fontSize, opts);
      }
      e.preventDefault();
    };

    container.addEventListener("touchstart", onTouchStart, { passive: true });
    container.addEventListener("touchmove", onTouchMove, { passive: false, capture: true });
    container.addEventListener("touchend", endPinch, { passive: true });
    container.addEventListener("touchcancel", endPinch, { passive: true });
    // Capture: run before xterm/ghostty wheel handlers so trackpad pinch adjusts font instead of scroll.
    container.addEventListener("wheel", onWheelTrackpad, { capture: true, passive: false });

    return () => {
      container.removeEventListener("touchstart", onTouchStart);
      container.removeEventListener("touchmove", onTouchMove, { capture: true });
      container.removeEventListener("touchend", endPinch);
      container.removeEventListener("touchcancel", endPinch);
      container.removeEventListener("wheel", onWheelTrackpad, { capture: true });
    };
  }, [
    ready,
    pinchZoomFont,
    sessionActive,
    fixedViewportGrid,
    minFontSize,
    maxFontSize,
    applyFontSizePx,
  ]);
}
