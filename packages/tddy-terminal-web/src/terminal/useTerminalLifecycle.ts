import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { Terminal, FitAddon } from "ghostty-web";
import { newGhosttyInstance } from "../lib/ghosttyInstance";
import { watchDevicePixelRatio } from "../lib/terminalDprWatch";
import type { GhosttyTerminalProps } from "./types";
import type { TerminalLoggers } from "./terminalLoggers";
import type { ContainerRef, TerminalRef } from "./terminalRefs";

type LifecycleProps = Pick<
  GhosttyTerminalProps,
  | "initialContent"
  | "onData"
  | "onResize"
  | "onBell"
  | "onTitleChange"
  | "onReady"
  | "onScroll"
  | "fixedViewportGrid"
  | "fontFamily"
  | "theme"
> & { cols: number; rows: number; fontSize: number; scrollback: number };

export interface TerminalLifecycleInput extends LifecycleProps {
  containerRef: ContainerRef;
  termRef: TerminalRef;
  /** Bumped on every write and clear — see `getBufferText`. */
  contentRevisionRef: MutableRefObject<number>;
  /** One OSC 52 recognizer per terminal instance: it sees every byte written. */
  osc52Tap: (data: string | Uint8Array) => void;
  loggers: TerminalLoggers;
}

/**
 * Opens the emulator in the container (and tears it down), fits it, reports its size, and writes
 * `initialContent` — before `ready` is signalled and again when the prop changes afterwards.
 */
export function useTerminalLifecycle({
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
}: TerminalLifecycleInput): { ready: boolean; fitAddonRef: MutableRefObject<FitAddon | null> } {
  const { logLife, logData, logResize } = loggers;
  // The latest `initialContent`, read by the open path (the setup effect has `[]` deps).
  const initialContentRef = useRef(initialContent);
  initialContentRef.current = initialContent;
  // The `initialContent` last written into the emulator, so the open path and the effect never
  // write the same value twice.
  const writtenInitialContentRef = useRef<string | undefined>(undefined);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  // Frame handle of the re-fit retry loop — at component scope because the effect's cleanup
  // must be able to cancel a loop armed inside `setup()` (which runs asynchronously, so the
  // cleanup can fire before setup's own locals exist).
  const refitRafRef = useRef(0);
  const [ready, setReady] = useState(false);
  const disposablesRef = useRef<{ dispose: () => void }[]>([]);
  const onScrollRef = useRef(onScroll);
  onScrollRef.current = onScroll;

  useEffect(() => {
    let isMounted = true;

    async function setup() {
      logLife("init cols=%d rows=%d fontSize=%d", cols, rows, fontSize);
      // Each terminal gets its own WASM instance rather than ghostty-web's process-wide one —
      // see `newGhosttyInstance` for why.
      const ghostty = await newGhosttyInstance();
      if (!isMounted || !containerRef.current) return;
      logLife("creating Terminal");

      const term = new Terminal({
        ghostty,
        cols,
        rows,
        fontSize,
        fontFamily,
        theme: theme ?? {
          background: "#1a1b26",
          foreground: "#a9b1d6",
        },
        scrollback,
      });

      termRef.current = term;

      const disposables: { dispose: () => void }[] = [];
      if (onData) {
        logLife("keyboard listener attached (onData)");
        disposables.push(
          term.onData((data) => {
            logData("onData %d chars %s", data.length, JSON.stringify(data.slice(0, 30)));
            // Native scroll-to-bottom.keystroke=true: any keystroke scrolls to the bottom first.
            const t = term as unknown as {
              scrollToBottom?: () => void;
              getViewportY?: () => number;
              scrollLines?: (n: number) => void;
            };
            if (t.scrollToBottom) {
              t.scrollToBottom();
            } else {
              const vy = t.getViewportY?.() ?? 0;
              if (vy !== 0 && t.scrollLines) {
                t.scrollLines(-vy);
              }
            }
            onData(data);
          })
        );
      }
      if (onResize) {
        disposables.push(
          term.onResize(({ cols: c, rows: r }) => {
            logResize("term.onResize cols=%d rows=%d", c, r);
            onResize({ cols: c, rows: r });
          })
        );
      }
      if (onBell) {
        disposables.push(term.onBell(onBell));
      }
      if (onTitleChange) {
        disposables.push(term.onTitleChange(onTitleChange));
      }
      disposables.push(
        term.onScroll((vy) => {
          onScrollRef.current?.(vy);
        })
      );
      disposablesRef.current = disposables;

      term.open(containerRef.current);

      // ghostty-web 0.4.0 reads devicePixelRatio once, at renderer
      // construction, and never again — a terminal mounted on one display
      // keeps that display's scale factor forever, so dragging the window
      // onto a screen with a different one leaves the canvas rendering at
      // the old scale, upscaled soft by the compositor. Sync the renderer
      // to the live value and watch for changes: render() detects the
      // backing-store mismatch a DPR change leaves and fully repaints at
      // the new scale. (devicePixelRatio is private in ghostty-web's types,
      // so the field is reached through a cast, as the library gives no API.)
      const syncRendererDpr = () => {
        const renderer = term.renderer;
        if (!renderer || !term.wasmTerm) return;
        const dpr = window.devicePixelRatio;
        const mutable = renderer as unknown as { devicePixelRatio: number };
        if (mutable.devicePixelRatio === dpr) return;
        mutable.devicePixelRatio = dpr;
        renderer.render(term.wasmTerm, true, term.viewportY, term);
      };
      syncRendererDpr();
      disposables.push({
        dispose: watchDevicePixelRatio(window, syncRendererDpr),
      });

      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);
      fitAddonRef.current = fitAddon;
      if (fixedViewportGrid) {
        term.resize(fixedViewportGrid.cols, fixedViewportGrid.rows);
      } else {
        fitAddon.fit();
        fitAddon.observeResize();
        // Belt-and-braces re-fit, watched on the container we render — and deliberately NOT
        // going through `fit()`: the addon's own observer debounces its fit through
        // `setTimeout`, and `fit()` itself clears its private `_isResizing` latch through
        // another one. A page the window manager treats as hidden (headless CI runs the AUT
        // in an occluded Electron window) throttles those timers indefinitely, so the latch
        // set by the mount-time fit never clears and every later fit() returns immediately —
        // the GhosttyTerminalShrink CI failure: container shrank to 300px, canvas frozen at
        // its fitted width, every fit() a no-op. Proposing the dimensions and resizing
        // through the public API sidesteps the latch; both self-guard on unchanged dims.
        // ResizeObserver callbacks run in the rendering steps, not on timers, so this fires
        // where the addon's timer-based path does not.
        const refit = (): boolean => {
          const addon = fitAddonRef.current;
          const term = termRef.current;
          if (!addon || !term) return true;
          const proposed = addon.proposeDimensions();
          // `getMetrics()` can transiently return nothing (it measures the live canvas) —
          // report false so the frame retry picks it up; the RO only fires while the size
          // actually changes, so a skipped proposal would otherwise be dropped for good.
          if (!proposed) return false;
          if (proposed.cols !== term.cols || proposed.rows !== term.rows) {
            term.resize(proposed.cols, proposed.rows);
          }
          return true;
        };
        let refitFrames = 0;
        const refitWithRetry = () => {
          if (refit() || refitFrames >= 30) return;
          refitFrames++;
          refitRafRef.current = requestAnimationFrame(refitWithRetry);
        };
        resizeObserverRef.current = new ResizeObserver(() => {
          cancelAnimationFrame(refitRafRef.current);
          refitFrames = 0;
          refitWithRetry();
        });
        resizeObserverRef.current.observe(containerRef.current);
      }

      // Proactively report the size on mount so the PTY is resized to match the screen
      // immediately — don't rely on the terminal only emitting onResize when the grid
      // changes (it may already match the default, so no event would fire).
      const container = containerRef.current;
      logResize(
        "mount report cols=%d rows=%d container=%dx%dpx fixedGrid=%o",
        term.cols,
        term.rows,
        container?.clientWidth ?? 0,
        container?.clientHeight ?? 0,
        fixedViewportGrid ?? null,
      );
      onResize?.({ cols: term.cols, rows: term.rows });

      logLife("term opened cols=%d rows=%d, calling onReady", term.cols, term.rows);
      // Content present at open is written before ready is signalled, so ready means "showing
      // what it was given". The `[initialContent, ready]` effect skips a value already written
      // here, so it renders once and its OSC 52 is copied once.
      const openContent = initialContentRef.current;
      if (openContent) {
        contentRevisionRef.current++;
        term.write(openContent);
        osc52Tap(openContent);
        writtenInitialContentRef.current = openContent;
      }
      setReady(true);
      onReady?.();
    }

    setup();

    return () => {
      isMounted = false;
      cancelAnimationFrame(refitRafRef.current);
      resizeObserverRef.current?.disconnect();
      resizeObserverRef.current = null;
      fitAddonRef.current?.dispose();
      fitAddonRef.current = null;
      disposablesRef.current.forEach((d) => d.dispose());
      disposablesRef.current = [];
      if (termRef.current) {
        termRef.current.dispose();
        termRef.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (!ready || !termRef.current) return;
    if (initialContent === writtenInitialContentRef.current) return;
    writtenInitialContentRef.current = initialContent;
    if (!initialContent) return;
    contentRevisionRef.current++;
    termRef.current.write(initialContent);
    osc52Tap(initialContent);
  }, [initialContent, ready]);

  return { ready, fitAddonRef };
}
