import type { ReactNode } from "react";

/**
 * Minimal safe Markdown: block headings, lists, and paragraphs. Inline/raw HTML is emitted as
 * React text nodes (escaped), not parsed as DOM — no script injection.
 *
 * Shared by the session workflow files panel and the worktree Code pane preview.
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

    if (trimmed.startsWith("# ")) {
      blocks.push(
        <h1 key={key()} className="text-xl font-semibold">
          {trimmed.slice(2)}
        </h1>,
      );
      i += 1;
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
