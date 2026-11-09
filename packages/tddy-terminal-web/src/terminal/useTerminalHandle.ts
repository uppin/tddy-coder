import { useImperativeHandle, useRef, type ForwardedRef, type MutableRefObject } from "react";
import type { Terminal } from "ghostty-web";
import { computeScrollbar } from "../lib/terminalScrollbar";
import { dWrite, type TerminalLoggers } from "./terminalLoggers";
import type { OnDataRef, TerminalRef } from "./terminalRefs";
import type { BufferLineInfo, GhosttyTerminalHandle } from "./types";

export interface TerminalHandleInput {
  ref: ForwardedRef<GhosttyTerminalHandle>;
  termRef: TerminalRef;
  onDataRef: OnDataRef;
  /** Bumped on every write and clear: the buffer's content changes only through those (and a
   *  resize, which `getBufferText` keys on separately). */
  contentRevisionRef: MutableRefObject<number>;
  osc52Tap: (data: string | Uint8Array) => void;
  loggers: TerminalLoggers;
  debugLogging: boolean;
  rows: number;
  minFontSize: number;
  maxFontSize: number;
  zoomVerbose: boolean;
  applyFontSizePx: (px: number, bounds?: { min: number; max: number }) => void;
  sendWheelSgrAt: (clientX: number, clientY: number, up: boolean) => boolean;
}

/** The imperative handle: scroll, buffer reads, input and write/clear/focus. */
export function useTerminalHandle({
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
}: TerminalHandleInput): void {
  const { logMouse, logWrite } = loggers;
  // Serializing a 50 000-line scrollback cell by cell on every poll pinned the WebView at a full
  // core even with no output arriving, so `getBufferText` memoizes on the content revision.
  const bufferTextCacheRef = useRef<{
    term: Terminal;
    revision: number;
    cols: number;
    rows: number;
    text: string;
  } | null>(null);

  // Note: onData is registered once in the setup useEffect above (line 97-104).
  // Do NOT re-register here — that would cause duplicate events for each keystroke.

  useImperativeHandle(
    ref,
    () => ({
      getViewportScrollOffset() {
        // Number of lines the viewport is scrolled up from the bottom of the
        // scrollback (0 = pinned to the latest output). Lets callers/tests
        // observe whether a scroll gesture actually moved the viewport.
        const t = termRef.current as unknown as { getViewportY?: () => number } | null;
        return t?.getViewportY?.() ?? 0;
      },
      scrollToTop() {
        // Scroll the viewport to the top of the scrollback so the user lands on the oldest
        // forward-filled history. No-op when scrollback is 0 (nothing to scroll through).
        const t = termRef.current as unknown as { scrollToTop?: () => void } | null;
        t?.scrollToTop?.();
      },
      scrollToBottom() {
        // Scroll the viewport to the bottom (latest output). Used after a page swap so the user
        // lands on the newest line of the just-populated older page (seamless continuation).
        const t = termRef.current as unknown as {
          getViewportY?: () => number;
          scrollLines?: (n: number) => void;
          scrollToBottom?: () => void;
        } | null;
        if (!t) return;
        if (t.scrollToBottom) {
          t.scrollToBottom();
          return;
        }
        // Fallback: scroll back to the bottom by negating the current viewport offset.
        const vy = t.getViewportY?.() ?? 0;
        if (vy !== 0 && t.scrollLines) {
          t.scrollLines(-vy);
        }
      },
      scrollToLine(line: number) {
        // Native scrollToLine uses Scrollbar offset space (0 = top of scrollback).
        // ghostty-web's scrollToLine sets viewportY directly (0 = bottom), so translate:
        // viewportY = scrollbackLength - line.
        const t = termRef.current as unknown as {
          scrollToLine?: (n: number) => void;
          getScrollbackLength?: () => number;
        } | null;
        if (!t?.scrollToLine) return;
        const scrollbackLength = t.getScrollbackLength?.() ?? 0;
        const viewportY = Math.max(0, Math.min(scrollbackLength, scrollbackLength - line));
        t.scrollToLine(viewportY);
      },
      scrollLines(amount: number) {
        // Relative scroll: positive = down toward latest, negative = up into scrollback.
        const t = termRef.current as unknown as { scrollLines?: (n: number) => void } | null;
        t?.scrollLines?.(amount);
      },
      getScrollbackLength() {
        // History lines excluding the active screen — used to compute absolute line from viewportY.
        const t = termRef.current as unknown as { getScrollbackLength?: () => number } | null;
        return t?.getScrollbackLength?.() ?? 0;
      },
      isPinnedToBottom() {
        // True when the viewport is at the latest output (not scrolled up into scrollback).
        const t = termRef.current as unknown as { getViewportY?: () => number } | null;
        return (t?.getViewportY?.() ?? 0) === 0;
      },
      getScrollbar() {
        const t = termRef.current;
        if (!t) return { total: rows, offset: 0, len: rows };
        const scrollbackLength = (t as unknown as { getScrollbackLength?: () => number })
          .getScrollbackLength?.() ?? 0;
        const viewportY = (t as unknown as { getViewportY?: () => number }).getViewportY?.() ?? 0;
        return computeScrollbar({ scrollbackLength, viewportY, rows: t.rows });
      },
      hasMouseTracking() {
        const t = termRef.current as unknown as { hasMouseTracking?: () => boolean } | null;
        return t?.hasMouseTracking?.() ?? false;
      },
      isAlternateScreen() {
        const t = termRef.current as unknown as {
          wasmTerm?: { isAlternateScreen?: () => boolean };
        } | null;
        return t?.wasmTerm?.isAlternateScreen?.() ?? false;
      },
      sendWheelSgr(e: WheelEvent) {
        if (e.ctrlKey) return;
        const sent = sendWheelSgrAt(e.clientX, e.clientY, e.deltaY < 0);
        logMouse(
          "wheel gate reported to TUI sent=%o deltaY=%d deltaMode=%d client=(%d,%d)",
          sent,
          e.deltaY,
          e.deltaMode,
          e.clientX,
          e.clientY,
        );
      },
      paste(text: string) {
        termRef.current?.paste(text);
      },
      sendKeystroke(data: string) {
        const t = termRef.current as unknown as {
          scrollToBottom?: () => void;
          getViewportY?: () => number;
          scrollLines?: (n: number) => void;
        } | null;
        if (t) {
          if (t.scrollToBottom) {
            t.scrollToBottom();
          } else {
            const vy = t.getViewportY?.() ?? 0;
            if (vy !== 0 && t.scrollLines) {
              t.scrollLines(-vy);
            }
          }
        }
        onDataRef.current?.(data);
      },
      setTerminalFontSize(px: number) {
        if (!Number.isFinite(px)) return;
        if (zoomVerbose) {
          console.info("[tddy][GhosttyTerminal] setTerminalFontSize (imperative)", { px });
        }
        applyFontSizePx(px, { min: minFontSize, max: maxFontSize });
      },
    write(data: string | Uint8Array) {
      if (debugLogging || dWrite.enabled) {
        const preview =
          typeof data === "string"
            ? JSON.stringify(data.slice(0, 40))
            : Array.from(data.slice(0, 24), (b) => b.toString(16).padStart(2, "0")).join(" ");
        logWrite("write %d %s %s", data.length, typeof data === "string" ? "chars" : "bytes", preview);
      }
      contentRevisionRef.current++;
      termRef.current?.write(data);
      osc52Tap(data);
    },
    clear() {
      contentRevisionRef.current++;
      termRef.current?.clear();
    },
    focus() {
      const term = termRef.current;
      if (!term) return;
      const textarea = term.textarea;
      if (textarea) {
        // ghostty-web routes keyboard input through this off-screen textarea — clicking the
        // terminal focuses it (see the library's canvas mousedown handler), and keydown bubbles
        // from it to the contenteditable host that owns the key listener. Focusing it directly is
        // what makes the terminal keyboard-ready. `term.focus()` instead focuses the host wrapper
        // and re-asserts that focus on a timer, so it can't be used to land focus on the input.
        textarea.removeAttribute("readonly");
        textarea.focus();
        return;
      }
      term.focus();
    },
    getBufferText() {
      const term = termRef.current;
      if (!term || !term.buffer?.active) return "";
      const revision = contentRevisionRef.current;
      const cached = bufferTextCacheRef.current;
      if (
        cached &&
        cached.term === term &&
        cached.revision === revision &&
        cached.cols === term.cols &&
        cached.rows === term.rows
      ) {
        return cached.text;
      }
      try {
        const buffer = term.buffer.active;
        const lines: string[] = [];
        for (let y = 0; y < buffer.length; y++) {
          const line = buffer.getLine(y);
          if (line) lines.push(line.translateToString());
        }
        const text = lines.join("");
        bufferTextCacheRef.current = { term, revision, cols: term.cols, rows: term.rows, text };
        return text;
      } catch {
        return "";
      }
    },
    getBufferLines() {
      const term = termRef.current;
      if (!term || !term.buffer?.active) return [];
      try {
        const buffer = term.buffer.active;
        const lines: BufferLineInfo[] = [];
        for (let y = 0; y < buffer.length; y++) {
          const line = buffer.getLine(y);
          if (!line) continue;
          const text = line.translateToString(true); // trimRight
          let hasInverse = false;
          for (let x = 0; x < line.length; x++) {
            const cell = line.getCell(x);
            if (cell && cell.isInverse()) {
              hasInverse = true;
              break;
            }
          }
          lines.push({ text, hasInverse });
        }
        return lines;
      } catch {
        return [];
      }
    },
    }),
    [applyFontSizePx, minFontSize, maxFontSize, zoomVerbose, sendWheelSgrAt]
  );
}
