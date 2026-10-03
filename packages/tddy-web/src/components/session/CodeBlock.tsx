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
  /** The pointer rests on an identifier: show its hover. */
  onHover?: (at: CodePosition) => void;
  /** One-based line to scroll into view and mark as the navigation target. */
  focusLine?: number;
};

/**
 * Read-only file preview body: syntax-highlights recognized code files and falls back to plain
 * monospace text for anything else. The theme follows the app's dark-mode class on the document.
 */
export function CodeBlock({ content, relPath }: CodeBlockProps) {
  // TODO(code-navigation): render each line and identifier token with its one-based byte
  // position, route ctrl/cmd-click to `onNavigate` and hover to `onHover`, and scroll `focusLine`
  // into view marked as the navigation target.
  const language = codeLanguageForPath(relPath);

  if (language === null) {
    return <pre className="whitespace-pre-wrap font-mono text-sm">{content}</pre>;
  }

  const isDark =
    typeof document !== "undefined" && document.documentElement.classList.contains("dark");

  return (
    <div data-testid="worktree-code-highlight">
      <SyntaxHighlighter
        language={language}
        style={isDark ? oneDark : oneLight}
        customStyle={{ margin: 0, background: "transparent", fontSize: "0.875rem" }}
        wrapLongLines
      >
        {content}
      </SyntaxHighlighter>
    </div>
  );
}
