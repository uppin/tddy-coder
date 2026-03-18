import React, {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { init, Terminal, FitAddon } from "ghostty-web";

export interface GhosttyTerminalTheme {
  background?: string;
  foreground?: string;
}

export interface GhosttyTerminalProps {
  initialContent?: string;
  cols?: number;
  rows?: number;
  fontSize?: number;
  fontFamily?: string;
  theme?: GhosttyTerminalTheme;
  onData?: (data: string) => void;
  onResize?: (size: { cols: number; rows: number }) => void;
  onBell?: () => void;
  onTitleChange?: (title: string) => void;
  onReady?: () => void;
  /** When true, log write/onData and lifecycle to console. */
  debugLogging?: boolean;
  /** When true, prevent terminal from receiving focus on pointer/touch events (e.g. mobile when keyboard closed). */
  preventFocusOnTap?: boolean;
}

export interface BufferLineInfo {
  /** Plain text of this line (trailing whitespace trimmed). */
  text: string;
  /** True if any cell on this line has reverse-video (isInverse). */
  hasInverse: boolean;
}

export interface GhosttyTerminalHandle {
  write(data: string | Uint8Array): void;
  clear(): void;
  focus(): void;
  getBufferText?(): string;
  /** Return per-line text + attribute info from the active buffer. */
  getBufferLines?(): BufferLineInfo[];
}

export const GhosttyTerminal = forwardRef<GhosttyTerminalHandle, GhosttyTerminalProps>(
  function GhosttyTerminal(
    {
      initialContent,
      cols = 80,
      rows = 24,
      fontSize = 14,
      fontFamily,
      theme,
      onData,
      onResize,
      onBell,
      onTitleChange,
      onReady,
      debugLogging = false,
      preventFocusOnTap = false,
    },
    ref
  ) {
    const log = debugLogging
      ? (...args: unknown[]) => console.log("[GhosttyTerminal]", ...args)
      : () => {};
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<Terminal | null>(null);
    const fitAddonRef = useRef<FitAddon | null>(null);
    const [ready, setReady] = useState(false);

    const disposablesRef = useRef<{ dispose: () => void }[]>([]);

    useEffect(() => {
      let isMounted = true;

      async function setup() {
        log("lifecycle: init");
        await init();
        if (!isMounted || !containerRef.current) return;
        log("lifecycle: creating Terminal");

        const term = new Terminal({
          cols,
          rows,
          fontSize,
          fontFamily,
          theme: theme ?? {
            background: "#1a1b26",
            foreground: "#a9b1d6",
          },
          scrollback: 0,
        });

        termRef.current = term;

        const disposables: { dispose: () => void }[] = [];
        if (onData) {
          console.log("[GhosttyTerminal] keyboard event listener attached (onData)");
          disposables.push(
            term.onData((data) => {
              log("dataflow: onData received", data.length, "chars", JSON.stringify(data.slice(0, 30)));
              onData(data);
            })
          );
        }
        if (onResize) {
          disposables.push(term.onResize(({ cols: c, rows: r }) => onResize({ cols: c, rows: r })));
        }
        if (onBell) {
          disposables.push(term.onBell(onBell));
        }
        if (onTitleChange) {
          disposables.push(term.onTitleChange(onTitleChange));
        }
        disposablesRef.current = disposables;

        term.open(containerRef.current);

        const fitAddon = new FitAddon();
        term.loadAddon(fitAddon);
        fitAddonRef.current = fitAddon;
        fitAddon.fit();
        fitAddon.observeResize();

        log("lifecycle: term opened, calling onReady");
        setReady(true);
        onReady?.();

        if (initialContent) {
          term.write(initialContent);
        }
      }

      setup();

      return () => {
        isMounted = false;
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
      if (ready && initialContent && termRef.current) {
        termRef.current.write(initialContent);
      }
    }, [initialContent, ready]);

    // Mouse/touch forwarding when hasMouseTracking (SGR sequences via onData)
    useEffect(() => {
      if (!ready || !containerRef.current || !termRef.current || !onData) return;
      const container = containerRef.current;
      const term = termRef.current;
      console.log("[GhosttyTerminal] mouse listeners attached to container", { ready, hasContainer: !!container, hasTerm: !!term });

      const toCellCoords = (offsetX: number, offsetY: number): { col: number; row: number } | null => {
        const rect = container.getBoundingClientRect();
        const c = term.cols;
        const r = term.rows;
        if (c <= 0 || r <= 0) return null;
        const cellW = rect.width / c;
        const cellH = rect.height / r;
        const col = Math.floor(offsetX / cellW) + 1;
        const row = Math.floor(offsetY / cellH) + 1;
        return { col: Math.max(1, Math.min(col, c)), row: Math.max(1, Math.min(row, r)) };
      };

      const sendSgr = (pb: number, col: number, row: number, release: boolean) => {
        const end = release ? "m" : "M";
        onData(`\x1b[<${pb};${col};${row}${end}`);
      };

      const onMouseDown = (e: MouseEvent) => {
        const coords = toCellCoords(e.offsetX, e.offsetY);
        const tracking = term.hasMouseTracking?.() ?? false;
        console.log("[GhosttyTerminal] mousedown", { col: coords?.col, row: coords?.row, offsetX: e.offsetX, offsetY: e.offsetY, hasMouseTracking: tracking });
        if (!tracking) return;
        if (coords) {
          log("mouse mousedown", "col=", coords.col, "row=", coords.row, "offsetX=", e.offsetX, "offsetY=", e.offsetY);
          sendSgr(0, coords.col, coords.row, false);
        }
      };
      const onMouseUp = (e: MouseEvent) => {
        const coords = toCellCoords(e.offsetX, e.offsetY);
        const tracking = term.hasMouseTracking?.() ?? false;
        console.log("[GhosttyTerminal] mouseup", { col: coords?.col, row: coords?.row, hasMouseTracking: tracking });
        if (!tracking) return;
        if (coords) {
          log("mouse mouseup", "col=", coords.col, "row=", coords.row, "offsetX=", e.offsetX, "offsetY=", e.offsetY);
          sendSgr(0, coords.col, coords.row, true);
        }
      };
      const onWheel = (e: WheelEvent) => {
        const rect = container.getBoundingClientRect();
        const offsetX = e.clientX - rect.left;
        const offsetY = e.clientY - rect.top;
        const coords = toCellCoords(offsetX, offsetY);
        const tracking = term.hasMouseTracking?.() ?? false;
        console.log("[GhosttyTerminal] wheel", { col: coords?.col, row: coords?.row, deltaY: e.deltaY, hasMouseTracking: tracking });
        if (!tracking) return;
        if (coords) {
          log("mouse wheel", "col=", coords.col, "row=", coords.row, "deltaY=", e.deltaY);
          const pb = e.deltaY < 0 ? 64 : 65;
          sendSgr(pb, coords.col, coords.row, false);
        }
        e.preventDefault();
      };

      // Capture-phase touch handlers run before preventFocus — ensures SGR is sent for interactive TUI
      const onTouchStartCapture = (e: TouchEvent) => {
        if (e.changedTouches.length === 0) return;
        const t = e.changedTouches[0];
        const rect = container.getBoundingClientRect();
        const offsetX = t.clientX - rect.left;
        const offsetY = t.clientY - rect.top;
        const coords = toCellCoords(offsetX, offsetY);
        const tracking = term.hasMouseTracking?.() ?? false;
        if (tracking && coords) {
          sendSgr(0, coords.col, coords.row, false);
        }
      };
      const onTouchEndCapture = (e: TouchEvent) => {
        if (e.changedTouches.length === 0) return;
        const t = e.changedTouches[0];
        const rect = container.getBoundingClientRect();
        const offsetX = t.clientX - rect.left;
        const offsetY = t.clientY - rect.top;
        const coords = toCellCoords(offsetX, offsetY);
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
    }, [ready, onData]);

    // Prevent focus on pointer/touch/click when preventFocusOnTap (e.g. mobile, keyboard closed).
    // Touch events still propagate to SGR forwarding (bubble phase) — preventDefault does not stop propagation.
    useEffect(() => {
      const container = containerRef.current;
      if (!container || !preventFocusOnTap) return;
      const preventFocus = (e: Event) => {
        e.preventDefault();
        // Blur after all handlers run — library may call focus() in its mousedown handler
        queueMicrotask(() => {
          const active = document.activeElement;
          if (active && container.contains(active)) {
            (active as HTMLElement).blur();
          }
        });
      };
      container.addEventListener("pointerdown", preventFocus, { capture: true });
      container.addEventListener("mousedown", preventFocus, { capture: true });
      container.addEventListener("touchstart", preventFocus, { capture: true, passive: false });
      container.addEventListener("click", preventFocus, { capture: true });
      return () => {
        container.removeEventListener("pointerdown", preventFocus, { capture: true });
        container.removeEventListener("mousedown", preventFocus, { capture: true });
        container.removeEventListener("touchstart", preventFocus, { capture: true, passive: false });
        container.removeEventListener("click", preventFocus, { capture: true });
      };
    }, [preventFocusOnTap]);

    // Set textarea readonly when preventFocusOnTap — prevents mobile keyboard from opening on tap
    useEffect(() => {
      if (!ready || !termRef.current) return;
      const textarea = termRef.current.textarea;
      if (!textarea) return;
      if (preventFocusOnTap) {
        textarea.setAttribute("readonly", "");
      } else {
        textarea.removeAttribute("readonly");
      }
    }, [ready, preventFocusOnTap]);

    // Note: onData is registered once in the setup useEffect above (line 97-104).
    // Do NOT re-register here — that would cause duplicate events for each keystroke.

    useImperativeHandle(ref, () => ({
      write(data: string | Uint8Array) {
        const len = typeof data === "string" ? data.length : data.length;
        log("dataflow: write", len, "bytes", typeof data === "string" ? JSON.stringify(data.slice(0, 40)) : "(Uint8Array)");
        termRef.current?.write(data);
      },
      clear() {
        termRef.current?.clear();
      },
      focus() {
        const term = termRef.current;
        if (!term) return;
        term.textarea?.removeAttribute("readonly");
        term.focus();
      },
      getBufferText() {
        const term = termRef.current;
        if (!term || !term.buffer?.active) return "";
        try {
          const buffer = term.buffer.active;
          let text = "";
          for (let y = 0; y < buffer.length; y++) {
            const line = buffer.getLine(y);
            if (line) text += line.translateToString();
          }
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
    }));

    return (
      <div
        data-testid="ghostty-terminal"
        ref={containerRef}
        style={{
          width: "100%",
          height: "100%",
          minHeight: 200,
        }}
      />
    );
  }
);
