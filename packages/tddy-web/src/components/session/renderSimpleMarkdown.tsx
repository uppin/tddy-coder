import { createElement, type ReactNode } from "react";

/** `#` … `######` followed by a space: the heading's level and its text. */
const HEADING = /^(#{1,6}) (.*)$/;

const HEADING_CLASS = ["text-xl", "text-lg", "text-base", "text-sm", "text-sm", "text-sm"];

/** A fence line opening or closing a code block: three backticks, optionally naming a language. */
const isFence = (line: string) => line.trimStart().startsWith("```");

/**
 * Minimal safe Markdown: block headings (`#` to `######`), fenced code blocks, lists, and
 * paragraphs. Inline/raw HTML is emitted as React text nodes (escaped), not parsed as DOM — no
 * script injection.
 *
 * Shared by the session workflow files panel, the worktree Code pane preview and its hover card.
 */
export function renderSimpleMarkdown(content: string): ReactNode {
  const lines = content.split(/\r?\n/);
  const blocks: ReactNode[] = [];
  let el = 0;
  const key = () => el++;

  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    const trimmed = line.trimEnd();

    if (trimmed === "") {
      i += 1;
      continue;
    }

    const heading = HEADING.exec(trimmed);
    if (heading !== null) {
      const level = heading[1].length;
      blocks.push(
        createElement(
          `h${level}`,
          { key: key(), className: `${HEADING_CLASS[level - 1]} font-semibold` },
          heading[2],
        ),
      );
      i += 1;
      continue;
    }

    if (isFence(trimmed)) {
      const code: string[] = [];
      i += 1;
      while (i < lines.length && !isFence(lines[i] ?? "")) {
        code.push(lines[i] ?? "");
        i += 1;
      }
      i += 1; // the closing fence, or past the end when it is missing
      blocks.push(
        <pre key={key()} className="overflow-x-auto font-mono text-sm">
          <code>{code.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    if (trimmed.startsWith("- ")) {
      const items: string[] = [];
      while (i < lines.length) {
        const L = lines[i] ?? "";
        const t = L.trimEnd();
        if (t === "") {
          break;
        }
        if (!t.startsWith("- ")) {
          break;
        }
        items.push(t.slice(2));
        i += 1;
      }
      blocks.push(
        <ul key={key()} className="list-disc pl-6">
          {items.map((t, j) => (
            <li key={j}>{t}</li>
          ))}
        </ul>,
      );
      continue;
    }

    blocks.push(
      <p key={key()} className="whitespace-pre-wrap">
        {trimmed}
      </p>,
    );
    i += 1;
  }

  return <div className="workflow-md-preview space-y-2">{blocks}</div>;
}
