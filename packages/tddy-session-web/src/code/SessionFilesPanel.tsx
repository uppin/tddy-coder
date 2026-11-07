import { useEffect, useState } from "react";

import { workflowPreviewKind } from "./sessionWorkflowPreview";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";

export type WorkflowFileRow = { basename: string };

export type SessionFilesPanelProps = {
  files: WorkflowFileRow[];
  fileContents: Record<string, string>;
  initialSelection?: string;
  /** When set with `onSelectBasename`, selection is controlled by the parent. */
  selectedBasename?: string;
  onSelectBasename?: (basename: string) => void;
};

/**
 * Session workflow file list + preview: Markdown renders to structured elements; YAML uses a
 * monospace syntax-highlight region for legibility.
 */
export function SessionFilesPanel({
  files,
  fileContents,
  initialSelection,
  selectedBasename: controlledSelected,
  onSelectBasename,
}: SessionFilesPanelProps) {
  const [internalSelected, setInternalSelected] = useState(
    initialSelection ?? files[0]?.basename ?? "",
  );
  const controlled = controlledSelected !== undefined;
  const selected = controlled ? controlledSelected! : internalSelected;

  useEffect(() => {
    if (controlled || files.length === 0) return;
    const next =
      initialSelection && files.some((f) => f.basename === initialSelection)
        ? initialSelection
        : (files[0]?.basename ?? "");
    setInternalSelected((prev) => (prev === next ? prev : next));
  }, [controlled, files, initialSelection]);

  const setSelected = (basename: string) => {
    if (controlled) {
      onSelectBasename?.(basename);
    } else {
      setInternalSelected(basename);
    }
  };

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
                data-selected={selected === f.basename ? true : undefined}
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
