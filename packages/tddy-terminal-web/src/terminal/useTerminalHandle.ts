import { useImperativeHandle, useRef, type ForwardedRef, type MutableRefObject } from "react";
import type { TerminalEngine } from "../engine/TerminalEngine";
import { computeScrollbar } from "../lib/terminalScrollbar";
import { dWrite, type TerminalLoggers } from "./terminalLoggers";
import type { OnDataRef, TerminalRef } from "./terminalRefs";
import type { GhosttyTerminalHandle } from "./types";

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
    term: TerminalEngine;
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
      getViewportTop() {
        // The buffer line on the viewport's first row, counted from the top (0 = oldest line;
        // `getScrollbackLength()` = pinned to the latest output). Lets callers/tests observe
        // whether a scroll gesture actually moved the viewport.
        return termRef.current?.viewportTop() ?? 0;
      },
      scrollToTop() {
        // Scroll the viewport to the top of the scrollback so the user lands on the oldest
        // forward-filled history. No-op when scrollback is 0 (nothing to scroll through).
        termRef.current?.scrollToTop();
      },
      scrollToBottom() {
        // Scroll the viewport to the bottom (latest output). Used after a page swap so the user
        // lands on the newest line of the just-populated older page (seamless continuation).
        termRef.current?.scrollToBottom();
      },
      scrollToLine(line: number) {
        // `line` counts from the top of the buffer (0 = top of scrollback).
        termRef.current?.scrollToLine(line);
      },
      scrollLines(amount: number) {
        // Relative scroll: positive = down toward latest, negative = up into scrollback.
        termRef.current?.scrollLines(amount);
      },
      getScrollbackLength() {
        // History lines excluding the active screen.
        return termRef.current?.scrollbackLength() ?? 0;
      },
      isPinnedToBottom() {
        // True when the viewport is at the latest output (not scrolled up into scrollback).
        const t = termRef.current;
        return !t || t.viewportTop() === t.scrollbackLength();
      },
      getScrollbar() {
        const t = termRef.current;
        if (!t) return { total: rows, offset: 0, len: rows };
        return computeScrollbar({
          scrollbackLength: t.scrollbackLength(),
          viewportTop: t.viewportTop(),
          rows: t.rows,
        });
      },
      hasMouseTracking() {
        return termRef.current?.mouseTracking() ?? false;
      },
      isAlternateScreen() {
        return termRef.current?.alternateScreen() ?? false;
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
        termRef.current?.scrollToBottom();
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
      termRef.current?.focus();
    },
    getBufferText() {
      const term = termRef.current;
      if (!term) return "";
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
      // Each line at its full grid width, as the emulator holds it: the engine hands lines back
      // trimmed, and consumers match across the padding.
      const text = term
        .readLines()
        .map((line) => line.text.padEnd(term.cols))
        .join("");
      bufferTextCacheRef.current = { term, revision, cols: term.cols, rows: term.rows, text };
      return text;
    },
    getBufferLines() {
      return termRef.current?.readLines() ?? [];
    },
    }),
    [applyFontSizePx, minFontSize, maxFontSize, zoomVerbose, sendWheelSgrAt]
  );
}
