import type { ReactNode } from "react";
import { useState } from "react";

import { workflowPreviewKind } from "./sessionWorkflowPreview";

export type WorkflowFileRow = { basename: string };

export type SessionFilesPanelProps = {
  files: WorkflowFileRow[];
  fileContents: Record<string, string>;
  initialSelection?: string;
};

/**
 * Minimal safe Markdown: block headings, lists, and paragraphs. Inline/raw HTML is emitted as
 * React text nodes (escaped), not parsed as DOM — no script injection.
 */
function renderSimpleMarkdown(content: string): ReactNode {
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

/**
 * Session workflow file list + preview: Markdown renders to structured elements; YAML uses a
 * monospace syntax-highlight region for legibility.
 */
export function SessionFilesPanel({
  files,
  fileContents,
  initialSelection,
}: SessionFilesPanelProps) {
  const [selected, setSelected] = useState(initialSelection ?? files[0]?.basename ?? "");

  const content = fileContents[selected] ?? "";
  const previewKind = workflowPreviewKind(selected);
  const isMd = previewKind === "markdown";
  const isYaml = previewKind === "yaml";

  return (
    <div className="flex min-h-0 flex-1 gap-4">
      <nav aria-label="Session workflow files" className="w-48 shrink-0">
        <ul>
          {files.map((f) => (
            <li key={f.basename}>
              <button
                type="button"
                onClick={() => setSelected(f.basename)}
                data-selected={selected === f.basename}
              >
                {f.basename}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <section
        aria-label="File preview"
        data-testid="session-file-preview"
        className="min-w-0 flex-1"
      >
        {isMd ? (
          renderSimpleMarkdown(content)
        ) : isYaml ? (
          <pre
            data-testid="yaml-syntax-highlight"
            className="overflow-auto rounded border border-border bg-muted/40 p-3 font-mono text-sm whitespace-pre-wrap"
          >
            {content}
          </pre>
        ) : (
          <pre className="font-mono text-sm whitespace-pre-wrap">{content}</pre>
        )}
      </section>
    </div>
  );
}
