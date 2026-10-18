import {
  createElement,
  memo,
  useEffect,
  useLayoutEffect,
  useRef,
  useSyncExternalStore,
  type CSSProperties,
  type MouseEvent,
  type MutableRefObject,
  type ReactNode,
} from "react";
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
import { NOTHING_DEFINABLE, type DefinableSource } from "./codeProbes";

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
   * Go to the definition of an identifier: on ctrl/cmd-click, and on a plain click on one
   * `definable` reports. Absent, the block stays a plain read-only preview.
   */
  onNavigate?: (at: CodePosition) => void;
  /** The pointer rests on the identifier at `at`: ask about it. Fires once the pointer has dwelt. */
  onHover?: (at: CodePosition) => void;
  /**
   * Which identifiers are known to have a definition: those are underlined and clickable. Each
   * identifier follows its own flag, so a change re-renders that identifier, not the file.
   */
  definable?: DefinableSource;
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

/**
 * What a pointer on an identifier does — read through a ref, so the highlighted tree never changes
 * with it.
 */
type IdentifierActions = {
  click: (at: CodePosition, definable: boolean, event: MouseEvent) => void;
  hover: (at: CodePosition) => void;
  leave: () => void;
};

/** What every identifier of one render reads: its actions and its definability. */
type IdentifierContext = {
  actions: MutableRefObject<IdentifierActions>;
  definable: DefinableSource;
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
  identifiers: IdentifierContext,
  key: number,
): ReactNode {
  if (node.type === "text") {
    return renderText(node.value ?? "", line, cursor, identifiers, key);
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
    {
      key,
      className: classNames.length > 0 ? classNames.join(" ") : undefined,
      style,
    },
    (node.children ?? []).map((child, i) =>
      renderNode(child, line, cursor, stylesheet, useInlineStyles, identifiers, i),
    ),
  );
}

function renderText(
  text: string,
  line: number,
  cursor: { bytes: number },
  identifiers: IdentifierContext,
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
      <Identifier
        key={`${at.column}`}
        line={at.line}
        column={at.column}
        text={match[0]}
        {...identifiers}
      />,
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
 * One identifier of the preview, at a one-based line and byte column. It reads its own definability,
 * so a probe answer re-renders it alone. `data-identifier-at` is how the hover card finds it.
 */
function Identifier({
  line,
  column,
  text,
  actions,
  definable,
}: { line: number; column: number; text: string } & IdentifierContext) {
  const isDefinable = useSyncExternalStore(definable.subscribe, () =>
    definable.isDefinable({ line, column }),
  );
  const at = { line, column };
  return (
    <span
      data-testid={`worktree-code-identifier-${line}-${column}`}
      data-identifier-at={`${line}:${column}`}
      data-definable={isDefinable ? "true" : undefined}
      className={isDefinable ? "cursor-pointer underline" : undefined}
      onClick={(e) => actions.current.click(at, isDefinable, e)}
      onMouseOver={() => actions.current.hover(at)}
      onMouseOut={() => actions.current.leave()}
    >
      {text}
    </span>
  );
}

type HighlightedCodeProps = {
  content: string;
  language: string;
  focusLine?: number;
  focusRef: MutableRefObject<HTMLSpanElement | null>;
  isDark: boolean;
} & IdentifierContext;

/**
 * The syntax-highlighted rows. Memoised: highlighting runs again only for new content, language,
 * target line, theme or definability source — never because a handler or one identifier's flag
 * changed.
 */
const HighlightedCode = memo(function HighlightedCode({
  content,
  language,
  focusLine,
  focusRef,
  isDark,
  actions,
  definable,
}: HighlightedCodeProps) {
  const identifiers: IdentifierContext = { actions, definable };
  return (
    <SyntaxHighlighter
      language={language}
      style={isDark ? oneDark : oneLight}
      customStyle={{
        margin: 0,
        background: "transparent",
        fontSize: "0.875rem",
      }}
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
                renderNode(
                  child,
                  line,
                  cursor,
                  stylesheet as Stylesheet,
                  useInlineStyles,
                  identifiers,
                  i,
                ),
              )}
            </span>
          );
        })
      }
    >
      {content}
    </SyntaxHighlighter>
  );
});

/**
 * Read-only file preview body: syntax-highlights recognized code files and falls back to plain
 * monospace text for anything else. The theme follows the app's dark-mode class on the document.
 *
 * Highlighted code is navigable when `onNavigate` / `onHover` are given: every line and every
 * identifier carries its one-based position. An identifier `definable` reports is underlined
 * (`data-definable="true"`) and a plain click on it navigates.
 */
export function CodeBlock({
  content,
  relPath,
  onNavigate,
  onHover,
  definable = NOTHING_DEFINABLE,
  focusLine,
}: CodeBlockProps) {
  const language = codeLanguageForPath(relPath);
  const dwellTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const focusRef = useRef<HTMLSpanElement | null>(null);
  const actions = useRef<IdentifierActions>({
    click: () => {},
    hover: () => {},
    leave: () => {},
  });

  const cancelDwell = () => {
    if (dwellTimer.current !== null) clearTimeout(dwellTimer.current);
    dwellTimer.current = null;
  };
  // A dwell begun on the previous file must not ask about the same position in this one.
  useEffect(() => cancelDwell, [content, relPath]);

  useEffect(() => {
    focusRef.current?.scrollIntoView({ block: "center" });
  }, [focusLine, content, relPath]);

  // Handlers fire only after a commit, so the identifiers read the latest ones set here.
  useLayoutEffect(() => {
    actions.current = {
      click: (at, isDefinable, event) => {
        if (!onNavigate) return;
        if (!(event.ctrlKey || event.metaKey || isDefinable)) return;
        event.preventDefault();
        cancelDwell();
        onNavigate(at);
      },
      hover: (at) => {
        if (!onHover) return;
        cancelDwell();
        dwellTimer.current = setTimeout(() => onHover(at), HOVER_DWELL_MS);
      },
      leave: cancelDwell,
    };
  });

  if (language === null) {
    return <pre className="whitespace-pre-wrap font-mono text-sm">{content}</pre>;
  }

  const isDark =
    typeof document !== "undefined" && document.documentElement.classList.contains("dark");

  return (
    <div data-testid="worktree-code-highlight">
      <HighlightedCode
        content={content}
        language={language}
        focusLine={focusLine}
        focusRef={focusRef}
        isDark={isDark}
        actions={actions}
        definable={definable}
      />
    </div>
  );
}
