import { useCallback, useMemo } from "react";
import type { Client } from "@connectrpc/connect";

import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import { WorktreeFileTree } from "./WorktreeFileTree";
import { createWorktreeFilesApi } from "./worktreeFilesApi";
import { workflowPreviewKind } from "./sessionWorkflowPreview";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";
import { CodeBlock } from "./CodeBlock";
import { createCodeNavigationApi, type CodePosition } from "./codeNavigationApi";
import { HoverCard, LocationListCard, NavigationNotice } from "./CodeNavigationOverlays";
import { useCodeNavigation, type SelectedFile } from "./useCodeNavigation";

export type WorktreeCodePaneProps = {
  client: Client<typeof WorktreeService>;
  sessionToken: string;
  projectId: string;
  /** The session's worktree root (`SessionEntry.repo_path`). */
  worktreePath: string;
  /**
   * `code_navigation.CodeNavigationService` on the same host as `client` — definition, references
   * and hover for the preview. Absent, the preview is read-only text.
   */
  navigationClient?: Client<typeof CodeNavigationService>;
};

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
  navigationClient,
}: WorktreeCodePaneProps) {
  const api = useMemo(
    () => createWorktreeFilesApi(client, { sessionToken, projectId, worktreePath }),
    [client, sessionToken, projectId, worktreePath],
  );
  const navigation = useMemo(
    () =>
      navigationClient
        ? createCodeNavigationApi(navigationClient, { sessionToken, projectId, worktreePath })
        : null,
    [navigationClient, sessionToken, projectId, worktreePath],
  );
  const nav = useCodeNavigation(api, navigation);
  const { selected, openFile } = nav;
  const handleSelectFile = useCallback((relPath: string) => openFile(relPath), [openFile]);

  // The index serves Rust only; offering ctrl-click and hover on other languages would ask it
  // questions it refuses.
  const navigable = navigation !== null && selected !== null && selected.relPath.endsWith(".rs");

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
      <div className="relative flex min-h-0 min-w-0 flex-1">
        <section
          data-testid="worktree-file-preview"
          aria-label="Worktree file preview"
          className="min-w-0 flex-1 overflow-auto p-3"
        >
          <FilePreview
            selected={selected}
            onNavigate={navigable ? nav.navigate : undefined}
            onHover={navigable ? nav.showHover : undefined}
          />
        </section>
        <div className="pointer-events-none absolute inset-x-3 bottom-3 flex flex-col gap-2">
          {nav.notice && <NavigationNotice notice={nav.notice} />}
          {nav.locations && <LocationListCard list={nav.locations} onOpen={nav.openLocation} />}
          {nav.hover && (
            <HoverCard
              hover={nav.hover}
              onShowReferences={nav.showReferences}
              onClose={nav.closeHover}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function FilePreview({
  selected,
  onNavigate,
  onHover,
}: {
  selected: SelectedFile | null;
  onNavigate?: (at: CodePosition) => void;
  onHover?: (at: CodePosition) => void;
}) {
  if (selected === null) {
    return <p className="text-sm text-muted-foreground">Select a file to preview</p>;
  }
  if (selected.error) {
    return <p className="text-sm text-destructive">{selected.content}</p>;
  }
  if (workflowPreviewKind(selected.relPath) === "markdown") {
    return <>{renderSimpleMarkdown(selected.content)}</>;
  }
  return (
    <CodeBlock
      content={selected.content}
      relPath={selected.relPath}
      onNavigate={onNavigate}
      onHover={onHover}
      focusLine={selected.focusLine}
    />
  );
}
