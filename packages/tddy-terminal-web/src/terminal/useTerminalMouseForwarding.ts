import { useEffect, type MutableRefObject } from "react";
import { clientPointToTerminalCell } from "../lib/terminalMouseCellCoords";
import type { TerminalLoggers } from "./terminalLoggers";
import type { ContainerRef, OnDataRef, TerminalRef } from "./terminalRefs";

/**
 * Mouse/touch forwarding when the TUI has mouse tracking on: SGR sequences via `onData`.
 * Reads `onDataRef` rather than depending on `onData`: callers pass an inline arrow, so an
 * `onData` dependency tore down and re-attached every listener on each parent render.
 */
export function useTerminalMouseForwarding({
  containerRef,
  termRef,
  onDataRef,
  wheelPinchAccumRef,
  ready,
  hasOnData,
  loggers,
}: {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  onDataRef: OnDataRef;
  wheelPinchAccumRef: MutableRefObject<number>;
  ready: boolean;
  hasOnData: boolean;
  loggers: TerminalLoggers;
}): void {
  const { logMouse } = loggers;
  useEffect(() => {
    if (!ready || !containerRef.current || !termRef.current || !hasOnData) return;
    const container = containerRef.current;
    const term = termRef.current;
    logMouse("mouse listeners attached ready=%o", ready);

    const canvasGridRect = () => {
      const canvas = term.element?.querySelector("canvas");
      if (!canvas) return null;
      const r = canvas.getBoundingClientRect();
      return { left: r.left, top: r.top, width: r.width, height: r.height };
    };

    const toCellCoords = (clientX: number, clientY: number): { col: number; row: number } | null => {
      const grid = canvasGridRect();
      if (!grid) return null;
      return clientPointToTerminalCell(clientX, clientY, grid, term.cols, term.rows);
    };

    const sendSgr = (pb: number, col: number, row: number, release: boolean) => {
      const end = release ? "m" : "M";
      onDataRef.current?.(`\x1b[<${pb};${col};${row}${end}`);
    };

    const onMouseDown = (e: MouseEvent) => {
      const coords = toCellCoords(e.clientX, e.clientY);
      const tracking = term.hasMouseTracking?.() ?? false;
      logMouse("mousedown col=%o row=%o tracking=%o", coords?.col, coords?.row, tracking);
      if (!tracking) return;
      if (coords) {
        sendSgr(0, coords.col, coords.row, false);
      }
    };
    const onMouseUp = (e: MouseEvent) => {
      const coords = toCellCoords(e.clientX, e.clientY);
      const tracking = term.hasMouseTracking?.() ?? false;
      logMouse("mouseup col=%o row=%o tracking=%o", coords?.col, coords?.row, tracking);
      if (!tracking) return;
      if (coords) {
        sendSgr(0, coords.col, coords.row, true);
      }
    };
    const onWheel = (e: WheelEvent) => {
      if (e.ctrlKey) {
        // Font zoom from trackpad pinch — handled in capture phase (pinch effect) without `onData`.
        return;
      }
      wheelPinchAccumRef.current = 0;

      const coords = toCellCoords(e.clientX, e.clientY);
      const tracking = term.hasMouseTracking?.() ?? false;
      logMouse("wheel col=%o row=%o deltaY=%d tracking=%o", coords?.col, coords?.row, e.deltaY, tracking);
      if (!tracking) return;
      if (coords) {
        const pb = e.deltaY < 0 ? 64 : 65;
        sendSgr(pb, coords.col, coords.row, false);
      }
      e.preventDefault();
    };

    // Capture-phase touch handlers run before preventFocus — ensures SGR is sent for interactive TUI
    const onTouchStartCapture = (e: TouchEvent) => {
      if (e.touches.length > 1) return;
      if (e.changedTouches.length === 0) return;
      const t = e.changedTouches[0];
      const coords = toCellCoords(t.clientX, t.clientY);
      const tracking = term.hasMouseTracking?.() ?? false;
      if (tracking && coords) {
        sendSgr(0, coords.col, coords.row, false);
      }
    };
    const onTouchEndCapture = (e: TouchEvent) => {
      if (e.changedTouches.length === 0) return;
      // Another finger still on the surface — skip release (pinch / multi-touch).
      if (e.touches.length > 0) return;
      const t = e.changedTouches[0];
      const coords = toCellCoords(t.clientX, t.clientY);
      const tracking = term.hasMouseTracking?.() ?? false;
      if (tracking && coords) {
        sendSgr(0, coords.col, coords.row, true);
      }
    };

    container.addEventListener("mousedown", onMouseDown);
    container.addEventListener("mouseup", onMouseUp);
    container.addEventListener("wheel", onWheel, { passive: false });
    container.addEventListener("touchstart", onTouchStartCapture, { capture: true });
    container.addEventListener("touchend", onTouchEndCapture, { capture: true });

    return () => {
      container.removeEventListener("mousedown", onMouseDown);
      container.removeEventListener("mouseup", onMouseUp);
      container.removeEventListener("wheel", onWheel);
      container.removeEventListener("touchstart", onTouchStartCapture, { capture: true });
      container.removeEventListener("touchend", onTouchEndCapture, { capture: true });
    };
  }, [ready, hasOnData]);
}
