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
}

export interface GhosttyTerminalHandle {
  write(data: string | Uint8Array): void;
  clear(): void;
  focus(): void;
  getBufferText?(): string;
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
    },
    ref
  ) {
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<Terminal | null>(null);
    const [ready, setReady] = useState(false);

    const disposablesRef = useRef<{ dispose: () => void }[]>([]);

    useEffect(() => {
      let isMounted = true;

      async function setup() {
        await init();
        if (!isMounted || !containerRef.current) return;

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
          disposables.push(term.onData(onData));
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

    useEffect(() => {
      if (onData && termRef.current) {
        termRef.current.onData(onData);
      }
    }, [onData]);

    useImperativeHandle(ref, () => ({
      write(data: string | Uint8Array) {
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
