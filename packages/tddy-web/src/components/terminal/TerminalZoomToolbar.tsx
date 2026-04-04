import React, { useEffect, useRef, useState } from "react";
import {
  canPitchIn,
  canPitchOut,
  DEFAULT_TERMINAL_FONT_MAX,
  DEFAULT_TERMINAL_FONT_MIN,
} from "../../lib/terminalZoom";
import {
  dispatchTerminalZoomBridge,
  isTerminalZoomDebugEnabled,
  parseTerminalFontSizeSyncDetail,
  TERMINAL_FONT_SIZE_SYNC_EVENT,
  type TerminalZoomBridgeAction,
} from "../../lib/terminalZoomBridge";

export interface TerminalZoomToolbarProps {
  /** Initial / reset font size (for reset action detail). */
  baselineFontSize: number;
  /** Optional override for min/max passed through the bridge. */
  minFontSize?: number;
  maxFontSize?: number;
}

function emit(action: TerminalZoomBridgeAction, baselineFontSize: number, min: number, max: number) {
  if (isTerminalZoomDebugEnabled()) {
    console.debug("[tddy][TerminalZoomToolbar] emit", { action, baselineFontSize, min, max });
  }
  dispatchTerminalZoomBridge({
    action,
    baselineFontSize,
    opts: { min, max },
  });
}

/** Limit Ctrl/Cmd +/-/0 so we do not steal shortcuts from other page inputs (e.g. browser chrome URL bar). */
function shouldHandleTerminalZoomKeys(): boolean {
  const el = document.activeElement;
  if (!el || el === document.body) return true;
  if (el.closest("[data-testid='ghostty-terminal']")) return true;
  if (el.closest("[data-testid='terminal-zoom-toolbar']")) return true;
  return false;
}

/**
 * Zoom controls for the embedded terminal (pitch in / out / reset).
 * Wired via window CustomEvent so a sibling GhosttyTerminal can subscribe without prop drilling.
 */
export function TerminalZoomToolbar({
  baselineFontSize,
  minFontSize = DEFAULT_TERMINAL_FONT_MIN,
  maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
}: TerminalZoomToolbarProps) {
  const [liveFontSize, setLiveFontSize] = useState(baselineFontSize);
  const liveRef = useRef(liveFontSize);
  liveRef.current = liveFontSize;
  const stepOpts = { min: minFontSize, max: maxFontSize };

  useEffect(() => {
    setLiveFontSize(baselineFontSize);
  }, [baselineFontSize]);

  useEffect(() => {
    const onSync = (ev: Event) => {
      const ce = ev as CustomEvent<unknown>;
      const fs = parseTerminalFontSizeSyncDetail(ce.detail);
      if (fs === null) return;
      setLiveFontSize(fs);
    };
    window.addEventListener(TERMINAL_FONT_SIZE_SYNC_EVENT, onSync as EventListener);
    return () =>
      window.removeEventListener(TERMINAL_FONT_SIZE_SYNC_EVENT, onSync as EventListener);
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      if (!shouldHandleTerminalZoomKeys()) return;
      const live = liveRef.current;
      const pitchOutDisabled = !canPitchOut(live, stepOpts);
      const pitchInDisabled = !canPitchIn(live, stepOpts);
      const key = e.key;
      if (key === "+" || key === "=") {
        if (pitchInDisabled) return;
        e.preventDefault();
        emit("pitch-in", baselineFontSize, minFontSize, maxFontSize);
        return;
      }
      if (key === "-" || key === "_") {
        if (pitchOutDisabled) return;
        e.preventDefault();
        emit("pitch-out", baselineFontSize, minFontSize, maxFontSize);
        return;
      }
      if (key === "0") {
        e.preventDefault();
        emit("reset", baselineFontSize, minFontSize, maxFontSize);
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [baselineFontSize, minFontSize, maxFontSize]);

  const pitchOutDisabled = !canPitchOut(liveFontSize, stepOpts);
  const pitchInDisabled = !canPitchIn(liveFontSize, stepOpts);

  return (
    <div
      role="toolbar"
      aria-label="Terminal zoom"
      data-testid="terminal-zoom-toolbar"
      style={{
        position: "absolute",
        top: 40,
        left: 8,
        zIndex: 102,
        display: "flex",
        gap: 4,
        pointerEvents: "auto",
      }}
    >
      <button
        type="button"
        data-testid="terminal-zoom-pitch-out"
        aria-label="Decrease terminal font size"
        disabled={pitchOutDisabled}
        style={{
          ...ZOOM_BTN_STYLE,
          opacity: pitchOutDisabled ? 0.45 : 1,
          cursor: pitchOutDisabled ? "not-allowed" : "pointer",
        }}
        onClick={() => emit("pitch-out", baselineFontSize, minFontSize, maxFontSize)}
      >
        −
      </button>
      <button
        type="button"
        data-testid="terminal-zoom-reset"
        aria-label="Reset terminal font size"
        style={ZOOM_BTN_STYLE}
        onClick={() => emit("reset", baselineFontSize, minFontSize, maxFontSize)}
      >
        0
      </button>
      <button
        type="button"
        data-testid="terminal-zoom-pitch-in"
        aria-label="Increase terminal font size"
        disabled={pitchInDisabled}
        style={{
          ...ZOOM_BTN_STYLE,
          opacity: pitchInDisabled ? 0.45 : 1,
          cursor: pitchInDisabled ? "not-allowed" : "pointer",
        }}
        onClick={() => emit("pitch-in", baselineFontSize, minFontSize, maxFontSize)}
      >
        +
      </button>
    </div>
  );
}

const ZOOM_BTN_STYLE: React.CSSProperties = {
  minWidth: 36,
  minHeight: 32,
  padding: "2px 8px",
  fontSize: 14,
  cursor: "pointer",
  backgroundColor: "rgba(0,0,0,0.65)",
  color: "#ddd",
  border: "1px solid #555",
  borderRadius: 4,
};
