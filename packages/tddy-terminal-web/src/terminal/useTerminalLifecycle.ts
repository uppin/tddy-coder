import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { xtermEngine } from "../engine/xtermEngine";
import type { EngineDisposable } from "../engine/TerminalEngine";
import type { GhosttyTerminalProps } from "./types";
import type { TerminalLoggers } from "./terminalLoggers";
import { fitTerminalToContainer } from "./terminalGeometry";
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
}: TerminalLifecycleInput): { ready: boolean } {
  const { logLife, logData, logResize } = loggers;
  // The latest `initialContent`, read by the open path (the setup effect has `[]` deps).
  const initialContentRef = useRef(initialContent);
  initialContentRef.current = initialContent;
  // The `initialContent` last written into the emulator, so the open path and the effect never
  // write the same value twice.
  const writtenInitialContentRef = useRef<string | undefined>(undefined);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  // Frame handle of the re-fit retry loop — at component scope because the effect's cleanup
  // must be able to cancel a loop armed inside `setup()` (which runs asynchronously, so the
  // cleanup can fire before setup's own locals exist).
  const refitRafRef = useRef(0);
  const [ready, setReady] = useState(false);
  const disposablesRef = useRef<EngineDisposable[]>([]);
  const onScrollRef = useRef(onScroll);
  onScrollRef.current = onScroll;

  useEffect(() => {
    let isMounted = true;

    async function setup() {
      logLife("init cols=%d rows=%d fontSize=%d", cols, rows, fontSize);
      const mountPoint = containerRef.current;
      if (!mountPoint) return;
      logLife("creating Terminal");

      const term = xtermEngine({
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

      const disposables: EngineDisposable[] = [];
      if (onData) {
        logLife("keyboard listener attached (onData)");
        disposables.push(
          term.onData((data) => {
            logData("onData %d chars %s", data.length, JSON.stringify(data.slice(0, 30)));
            // Native scroll-to-bottom.keystroke=true: any keystroke scrolls to the bottom first.
            term.scrollToBottom();
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
        term.onScroll((top) => {
          onScrollRef.current?.(top);
        })
      );
      disposablesRef.current = disposables;

      await term.open(mountPoint);
      if (!isMounted || !containerRef.current) {
        term.dispose();
        return;
      }
      termRef.current = term;

      if (fixedViewportGrid) {
        term.resize(fixedViewportGrid.cols, fixedViewportGrid.rows);
      } else {
        fitTerminalToContainer(term);
        // Belt-and-braces re-fit, watched on the container we render. A page the window manager
        // treats as hidden (headless CI runs the AUT in an occluded Electron window) throttles
        // timers indefinitely, so the re-fit is driven by a ResizeObserver, whose callbacks run in
        // the rendering steps, not on timers. Proposing the dimensions and resizing through the
        // engine self-guards on unchanged dims.
        const refit = (): boolean => {
          const engine = termRef.current;
          if (!engine) return true;
          // The engine can transiently fail to measure (it measures the live screen) — report
          // false so the frame retry picks it up; the RO only fires while the size actually
          // changes, so a skipped proposal would otherwise be dropped for good.
          if (!engine.proposeDimensions()) return false;
          fitTerminalToContainer(engine);
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

  return { ready };
}
