import React, { forwardRef, useMemo, useRef } from "react";
import { copyToClipboard } from "./lib/clipboard";
import { createOsc52Recognizer } from "./lib/osc52";
import { DEFAULT_TERMINAL_FONT_MAX, DEFAULT_TERMINAL_FONT_MIN } from "./lib/terminalZoom";
import { isTerminalZoomDebugEnabled } from "./lib/terminalZoomBridge";
import { createTerminalLoggers } from "./terminal/terminalLoggers";
import type { TerminalRef } from "./terminal/terminalRefs";
import type { GhosttyTerminalHandle, GhosttyTerminalProps } from "./terminal/types";
import { useTerminalHandle } from "./terminal/useTerminalHandle";
import { useTerminalFontSize } from "./terminal/useTerminalFontSize";
import { useTerminalLifecycle } from "./terminal/useTerminalLifecycle";
import { useDesktopKeyForwarding, useImeEscapeForwarding } from "./terminal/useTerminalKeyForwarding";
import { useTerminalMouseForwarding } from "./terminal/useTerminalMouseForwarding";
import { useTerminalPinchZoom } from "./terminal/useTerminalPinchZoom";
import {
  usePreventFocusOnTap,
  useTapAsClick,
  useTerminalTextareaAttrs,
} from "./terminal/useTerminalTouchFocus";
import { useSendWheelSgrAt, useTouchScrollRoutes } from "./terminal/useTerminalTouchScroll";

export type {
  BufferLineInfo,
  GhosttyTerminalHandle,
  GhosttyTerminalProps,
  GhosttyTerminalTheme,
} from "./terminal/types";

export const GhosttyTerminal = forwardRef<GhosttyTerminalHandle, GhosttyTerminalProps>(
  function GhosttyTerminal(
    {
      initialContent,
      cols = 80,
      rows = 24,
      fontSize = 14,
      minFontSize = DEFAULT_TERMINAL_FONT_MIN,
      maxFontSize = DEFAULT_TERMINAL_FONT_MAX,
      fontFamily,
      theme,
      onData,
      onResize,
      onBell,
      onTitleChange,
      onReady,
      onScroll,
      debugLogging = false,
      preventFocusOnTap = false,
      sessionActive = true,
      pinchZoomFont = true,
      containerMinHeightPx,
      fixedViewportGrid,
      scrollback = 0,
      testId = "ghostty-terminal",
    },
    ref
  ) {
    const loggers = createTerminalLoggers(debugLogging);
    const zoomVerbose = debugLogging || isTerminalZoomDebugEnabled();
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef: TerminalRef = useRef(null);
    // Bumped on every write and clear: the buffer's content changes only through those (and a
    // resize, which `getBufferText` keys on separately).
    const contentRevisionRef = useRef(0);

    // One OSC 52 recognizer per terminal instance: it sees every byte written, and copies clipboard writes.
    const osc52Tap = useMemo(
      () => createOsc52Recognizer((text) => void copyToClipboard(text)),
      [],
    );

    /** Accumulates `deltaY` for trackpad pinch (Ctrl+wheel); touch pinch uses separate logic. */
    const wheelPinchAccumRef = useRef(0);
    const onDataRef = useRef(onData);
    onDataRef.current = onData;

    const { ready } = useTerminalLifecycle({
      containerRef,
      termRef,
      contentRevisionRef,
      osc52Tap,
      loggers,
      initialContent,
      cols,
      rows,
      fontSize,
      fontFamily,
      theme,
      scrollback,
      fixedViewportGrid,
      onData,
      onResize,
      onBell,
      onTitleChange,
      onReady,
      onScroll,
    });

    const { applyFontSizePx, displayFontSize } = useTerminalFontSize({
      containerRef,
      termRef,
      ready,
      sessionActive,
      fontSize,
      minFontSize,
      maxFontSize,
      fixedViewportGrid,
      zoomVerbose,
    });

    useDesktopKeyForwarding({ containerRef, onDataRef, sessionActive });

    useTerminalPinchZoom({
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
    });

    const sendWheelSgrAt = useSendWheelSgrAt({ termRef, onDataRef });

    useTouchScrollRoutes({
      containerRef,
      termRef,
      onDataRef,
      sendWheelSgrAt,
      sessionActive,
      fixedViewportGrid,
      loggers,
    });

    useTerminalTextareaAttrs({ termRef, ready, sessionActive, preventFocusOnTap });

    useTerminalMouseForwarding({
      containerRef,
      termRef,
      onDataRef,
      wheelPinchAccumRef,
      ready,
      hasOnData: !!onData,
      loggers,
    });

    usePreventFocusOnTap({ containerRef, preventFocusOnTap });
    useTapAsClick({ containerRef, termRef, preventFocusOnTap, loggers });
    useImeEscapeForwarding({ containerRef, termRef, onDataRef, ready, sessionActive });

    useTerminalHandle({
      ref,
      termRef,
      onDataRef,
      contentRevisionRef,
      osc52Tap,
      loggers,
      debugLogging,
      rows,
      minFontSize,
      maxFontSize,
      zoomVerbose,
      applyFontSizePx,
      sendWheelSgrAt,
    });

    return (
      <div
        data-testid={testId}
        data-terminal-font-size={String(displayFontSize)}
        data-session-active={sessionActive ? "true" : "false"}
        // Set once the emulator is open in this element: it takes focus and input from then on.
        data-terminal-ready={ready ? "true" : "false"}
        aria-disabled={sessionActive ? undefined : true}
        ref={containerRef}
        style={{
          width: "100%",
          height: "100%",
          // The canvas has a fixed pixel width. Without these, the container's
          // min-content width is the canvas' fitted width — a flex-hosted pane
          // (a `flex: 1` ancestor without min-width: 0) then refuses to shrink
          // below it when the window collapses, the FitAddon's ResizeObserver
          // sees no change, and the grid goes stale, clipping text at the old
          // edge. Clipping overflow here is the desired behavior: the pane
          // shrinks, the observer fires, and the grid re-fits to the parent.
          minWidth: 0,
          overflow: "hidden",
          minHeight: containerMinHeightPx !== undefined ? containerMinHeightPx : 200,
          ...(sessionActive
            ? {}
            : { opacity: 0.55, pointerEvents: "none" as const }),
        }}
      />
    );
  }
);
