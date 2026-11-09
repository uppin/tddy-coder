import { useEffect, useState, type MutableRefObject, type RefObject } from "react";
import type { GhosttyTerminalHandle } from "../terminal/types";

/** What the hidden probe elements show — see `TerminalProbes`. */
export interface TerminalMirrorValues {
  bufferText: string;
  olderBufferText: string;
  highlightedLine: string;
  pageViewportY: number;
  liveViewportY: number;
  liveScrollbackLength: number;
  pageScrollbar: string;
  liveScrollbar: string;
}

/**
 * Mirrors the two panes' buffer text and viewport position into React state, polled every 200 ms
 * and refreshed on demand through `refreshMirrors`. Component tests read them back through hidden
 * elements.
 */
export function useTerminalMirrors({
  termRef,
  olderTermRef,
  streamedTextRef,
  showBufferTextForTest,
}: {
  termRef: RefObject<GhosttyTerminalHandle | null>;
  olderTermRef: RefObject<GhosttyTerminalHandle | null>;
  streamedTextRef: MutableRefObject<string>;
  showBufferTextForTest: boolean;
}): { mirrors: TerminalMirrorValues; refreshMirrors: () => void } {
  const [bufferText, setBufferText] = useState("");
  const [olderBufferText, setOlderBufferText] = useState("");
  const [highlightedLine, setHighlightedLine] = useState("");
  // Viewport position mirrors for the two panes (lines up from the bottom). Surfaced through hidden
  // elements so component tests can assert scrollToLine gives full control of the viewport.
  const [pageViewportY, setPageViewportY] = useState(0);
  const [liveViewportY, setLiveViewportY] = useState(0);
  const [liveScrollbackLength, setLiveScrollbackLength] = useState(0);
  const [pageScrollbar, setPageScrollbar] = useState("");
  const [liveScrollbar, setLiveScrollbar] = useState("");

  const refreshMirrors = () => {
    setPageViewportY(olderTermRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveViewportY(termRef.current?.getViewportScrollOffset?.() ?? 0);
    setLiveScrollbackLength(termRef.current?.getScrollbackLength?.() ?? 0);
    const pageSb = olderTermRef.current?.getScrollbar?.();
    setPageScrollbar(pageSb ? `${pageSb.total},${pageSb.offset},${pageSb.len}` : "");
    const liveSb = termRef.current?.getScrollbar?.();
    setLiveScrollbar(liveSb ? `${liveSb.total},${liveSb.offset},${liveSb.len}` : "");
  };

  useEffect(() => {
    const interval = setInterval(() => {
      // Ghostty's parsed buffer is the clean text; the raw streamed ANSI is the fallback for the
      // E2E probe, whose assertion is "bytes arrived" and which runs before a frame is painted.
      const fromBuffer = termRef.current?.getBufferText?.() ?? "";
      setBufferText(showBufferTextForTest ? fromBuffer || streamedTextRef.current : fromBuffer);
      setOlderBufferText(olderTermRef.current?.getBufferText?.() ?? "");
      if (showBufferTextForTest) {
        const lines = termRef.current?.getBufferLines?.() ?? [];
        const inverseLine = lines.find((l) => l.hasInverse && l.text.trim().length > 0);
        if (inverseLine) setHighlightedLine(inverseLine.text);
      }
      refreshMirrors();
    }, 200);
    return () => clearInterval(interval);
  }, [showBufferTextForTest]);

  return {
    mirrors: {
      bufferText,
      olderBufferText,
      highlightedLine,
      pageViewportY,
      liveViewportY,
      liveScrollbackLength,
      pageScrollbar,
      liveScrollbar,
    },
    refreshMirrors,
  };
}
