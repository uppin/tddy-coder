import React, {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { init, Terminal } from "ghostty-web";

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
    },
    ref
  ) {
    const log = debugLogging
      ? (...args: unknown[]) => console.log("[GhosttyTerminal]", ...args)
      : () => {};
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<Terminal | null>(null);
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
        termRef.current?.focus();
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
        style={{ width: "100%", height: "100%", minHeight: 200 }}
      />
    );
  }
);
