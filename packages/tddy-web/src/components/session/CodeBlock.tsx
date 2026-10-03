import { createElement, useEffect, useRef, type CSSProperties, type MouseEvent, type ReactNode } from "react";
import { PrismLight as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark, oneLight } from "react-syntax-highlighter/dist/esm/styles/prism";
import bash from "react-syntax-highlighter/dist/esm/languages/prism/bash";
import c from "react-syntax-highlighter/dist/esm/languages/prism/c";
import cpp from "react-syntax-highlighter/dist/esm/languages/prism/cpp";
import css from "react-syntax-highlighter/dist/esm/languages/prism/css";
import go from "react-syntax-highlighter/dist/esm/languages/prism/go";
import java from "react-syntax-highlighter/dist/esm/languages/prism/java";
import json from "react-syntax-highlighter/dist/esm/languages/prism/json";
import jsx from "react-syntax-highlighter/dist/esm/languages/prism/jsx";
import markup from "react-syntax-highlighter/dist/esm/languages/prism/markup";
import python from "react-syntax-highlighter/dist/esm/languages/prism/python";
import ruby from "react-syntax-highlighter/dist/esm/languages/prism/ruby";
import rust from "react-syntax-highlighter/dist/esm/languages/prism/rust";
import toml from "react-syntax-highlighter/dist/esm/languages/prism/toml";
import tsx from "react-syntax-highlighter/dist/esm/languages/prism/tsx";
import yaml from "react-syntax-highlighter/dist/esm/languages/prism/yaml";

import { codeLanguageForPath } from "../../lib/codeLanguage";
import type { CodePosition } from "./codeNavigationApi";

SyntaxHighlighter.registerLanguage("bash", bash);
SyntaxHighlighter.registerLanguage("c", c);
SyntaxHighlighter.registerLanguage("cpp", cpp);
SyntaxHighlighter.registerLanguage("css", css);
SyntaxHighlighter.registerLanguage("go", go);
SyntaxHighlighter.registerLanguage("java", java);
SyntaxHighlighter.registerLanguage("json", json);
SyntaxHighlighter.registerLanguage("jsx", jsx);
SyntaxHighlighter.registerLanguage("markup", markup);
SyntaxHighlighter.registerLanguage("python", python);
SyntaxHighlighter.registerLanguage("ruby", ruby);
SyntaxHighlighter.registerLanguage("rust", rust);
SyntaxHighlighter.registerLanguage("toml", toml);
SyntaxHighlighter.registerLanguage("tsx", tsx);
SyntaxHighlighter.registerLanguage("yaml", yaml);

export type CodeBlockProps = {
  content: string;
  relPath: string;
  /**
   * Ctrl/cmd-click on an identifier: go to its definition. Absent, the block stays a plain
   * read-only preview.
   */
  onNavigate?: (at: CodePosition) => void;
  /** The pointer rests on an identifier: show its hover. Fires once the pointer has dwelt on it. */
  onHover?: (at: CodePosition) => void;
  /** One-based line to scroll into view and mark as the navigation target. */
  focusLine?: number;
};

/** How long the pointer must rest on an identifier before its hover is requested. */
const HOVER_DWELL_MS = 250;

const IDENTIFIER = /[\p{L}_][\p{L}\p{N}_]*/gu;

const utf8Encoder = new TextEncoder();
const utf8Length = (text: string) => utf8Encoder.encode(text).length;

/** The subset of a refractor/hast node the row renderer reads. */
type SyntaxNode = {
  type: "element" | "text";
  tagName?: string;
  value?: string;
  properties?: { className?: string[]; style?: CSSProperties };
  children?: SyntaxNode[];
};

type Stylesheet = Record<string, CSSProperties>;

type Handlers = {
  navigate?: (at: CodePosition) => void;
  hoverIdentifier: (at: CodePosition, event: MouseEvent) => void;
  leaveIdentifier: () => void;
};

/**
 * Render one syntax-tree node. `cursor.bytes` is the UTF-8 byte offset of the node's first
 * character within its line, so identifier columns are one-based *byte* columns — the daemon's
 * coordinates — not JS UTF-16 string indices.
 */
function renderNode(
  node: SyntaxNode,
  line: number,
  cursor: { bytes: number },
  stylesheet: Stylesheet,
  useInlineStyles: boolean,
  handlers: Handlers,
  key: number,
): ReactNode {
  if (node.type === "text") {
    return renderText(node.value ?? "", line, cursor, handlers, key);
  }
  const classNames = node.properties?.className ?? [];
  const style = useInlineStyles
    ? {
        ...classNames.reduce<CSSProperties>((acc, name) => ({ ...acc, ...stylesheet[name] }), {}),
        ...node.properties?.style,
      }
    : node.properties?.style;
  return createElement(
    node.tagName ?? "span",
    { key, className: classNames.length > 0 ? classNames.join(" ") : undefined, style },
    (node.children ?? []).map((child, i) =>
      renderNode(child, line, cursor, stylesheet, useInlineStyles, handlers, i),
    ),
  );
}

function renderText(
  text: string,
  line: number,
  cursor: { bytes: number },
  handlers: Handlers,
  key: number,
): ReactNode {
  const parts: ReactNode[] = [];
  let consumed = 0;
  for (const match of text.matchAll(IDENTIFIER)) {
    const before = text.slice(consumed, match.index);
    cursor.bytes += utf8Length(before);
    if (before !== "") parts.push(before);
    const at = { line, column: cursor.bytes + 1 };
    parts.push(
      <span
        key={`${at.column}`}
        data-testid={`worktree-code-identifier-${at.line}-${at.column}`}
        onClick={(e) => {
          if ((e.ctrlKey || e.metaKey) && handlers.navigate) {
            e.preventDefault();
            handlers.navigate(at);
          }
        }}
        onMouseOver={(e) => handlers.hoverIdentifier(at, e)}
        onMouseOut={handlers.leaveIdentifier}
      >
        {match[0]}
      </span>,
    );
    cursor.bytes += utf8Length(match[0]);
    consumed = match.index + match[0].length;
  }
  const rest = text.slice(consumed);
  cursor.bytes += utf8Length(rest);
  if (rest !== "") parts.push(rest);
  return <span key={key}>{parts}</span>;
}

/**
 * Read-only file preview body: syntax-highlights recognized code files and falls back to plain
 * monospace text for anything else. The theme follows the app's dark-mode class on the document.
 *
 * Highlighted code is navigable when `onNavigate` / `onHover` are given: every line and every
 * identifier carries its one-based position.
 */
export function CodeBlock({ content, relPath, onNavigate, onHover, focusLine }: CodeBlockProps) {
  const language = codeLanguageForPath(relPath);
  const dwellTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const focusRef = useRef<HTMLSpanElement | null>(null);

  const cancelDwell = () => {
    if (dwellTimer.current !== null) clearTimeout(dwellTimer.current);
    dwellTimer.current = null;
  };
  useEffect(() => cancelDwell, []);

  useEffect(() => {
    focusRef.current?.scrollIntoView({ block: "center" });
  }, [focusLine, content, relPath]);

  if (language === null) {
    return <pre className="whitespace-pre-wrap font-mono text-sm">{content}</pre>;
  }

  const isDark =
    typeof document !== "undefined" && document.documentElement.classList.contains("dark");

  const handlers: Handlers = {
    navigate: onNavigate,
    hoverIdentifier: (at) => {
      if (!onHover) return;
      cancelDwell();
      dwellTimer.current = setTimeout(() => onHover(at), HOVER_DWELL_MS);
    },
    leaveIdentifier: cancelDwell,
  };

  return (
    <div data-testid="worktree-code-highlight">
      <SyntaxHighlighter
        language={language}
        style={isDark ? oneDark : oneLight}
        customStyle={{ margin: 0, background: "transparent", fontSize: "0.875rem" }}
        wrapLongLines
        renderer={({ rows, stylesheet, useInlineStyles }) =>
          (rows as SyntaxNode[]).map((row, index) => {
            const line = index + 1;
            const isTarget = line === focusLine;
            const cursor = { bytes: 0 };
            return (
              <span
                key={line}
                ref={isTarget ? focusRef : undefined}
                data-testid={`worktree-code-line-${line}`}
                data-navigation-target={isTarget ? "true" : undefined}
                className={isTarget ? "bg-primary/15" : undefined}
              >
                {(row.children ?? []).map((child, i) =>
                  renderNode(child, line, cursor, stylesheet as Stylesheet, useInlineStyles, handlers, i),
                )}
              </span>
            );
          })
        }
      >
        {content}
      </SyntaxHighlighter>
    </div>
  );
}
