import { useCallback, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";

import type { ConnectionService } from "../../gen/connection_pb";
import { WorktreeFileTree } from "./WorktreeFileTree";
import { createWorktreeFilesApi } from "./worktreeFilesApi";
import { workflowPreviewKind } from "./sessionWorkflowPreview";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";
import { CodeBlock } from "./CodeBlock";

export type WorktreeCodePaneProps = {
  client: Client<typeof ConnectionService>;
  sessionToken: string;
  projectId: string;
  /** The session's worktree root (`SessionEntry.repo_path`). */
  worktreePath: string;
};

type SelectedFile = { relPath: string; content: string; error: boolean };

/**
 * Split Code pane: a lazy worktree directory tree on the left and a read-only file preview on the
 * right. Markdown renders as sanitized markup, everything else as monospace text. File content is
 * fetched on demand when a file node is selected.
 */
export function WorktreeCodePane({
  client,
  sessionToken,
  projectId,
  worktreePath,
}: WorktreeCodePaneProps) {
  const api = useMemo(
    () => createWorktreeFilesApi(client, { sessionToken, projectId, worktreePath }),
    [client, sessionToken, projectId, worktreePath],
  );

  const [selected, setSelected] = useState<SelectedFile | null>(null);

  const handleSelectFile = useCallback(
    (relPath: string) => {
      void api
        .readFile(relPath)
        .then((res) => setSelected({ relPath, content: res.contentUtf8, error: false }))
        .catch((e: unknown) => {
          const message = e instanceof Error ? e.message : "Failed to read file";
          setSelected({ relPath, content: message, error: true });
        });
    },
    [api],
  );

  const previewKind =
    selected && !selected.error ? workflowPreviewKind(selected.relPath) : "plain";

  return (
    <div
      data-testid="worktree-code-pane"
      className="flex h-full min-h-0 min-w-0 flex-1 overflow-hidden"
    >
      <div className="w-56 shrink-0 border-r border-border">
        <WorktreeFileTree
          api={api}
          selectedRelPath={selected?.relPath ?? null}
          onSelectFile={handleSelectFile}
        />
      </div>
      <section
        data-testid="worktree-file-preview"
        aria-label="Worktree file preview"
        className="min-w-0 flex-1 overflow-auto p-3"
      >
        {selected === null ? (
          <p className="text-sm text-muted-foreground">Select a file to preview</p>
        ) : selected.error ? (
          <p className="text-sm text-destructive">{selected.content}</p>
        ) : previewKind === "markdown" ? (
          renderSimpleMarkdown(selected.content)
        ) : (
          <CodeBlock content={selected.content} relPath={selected.relPath} />
        )}
      </section>
    </div>
  );
}
