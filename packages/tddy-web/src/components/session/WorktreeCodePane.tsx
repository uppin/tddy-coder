import { useCallback, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";

import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import { WorktreeFileTree } from "./WorktreeFileTree";
import { createWorktreeFilesApi } from "./worktreeFilesApi";
import { workflowPreviewKind } from "./sessionWorkflowPreview";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";
import { CodeBlock } from "./CodeBlock";
import type { CodePosition } from "./codeNavigationApi";
import { HoverCard, LocationListCard, NavigationNotice } from "./CodeNavigationOverlays";
import { useCodeNavigation, type SelectedFile } from "./useCodeNavigation";
import { useCodeSession } from "./useCodeSession";
import { currentPhaseHeadline, type CodeIndexStatus } from "./useCodeIndexProgress";
import { createRestructurePlanApi, isRestructurePlanFile } from "./restructurePlanApi";
import { RestructurePlanDialog } from "./RestructurePlanDialog";
import { CodeIndexStatusStrip } from "./CodeIndexStatusStrip";

export type WorktreeCodePaneProps = {
  client: Client<typeof WorktreeService>;
  sessionToken: string;
  projectId: string;
  /** The session's worktree root (`SessionEntry.repo_path`). */
  worktreePath: string;
  /**
   * `code_navigation.CodeNavigationService` on the same host as `client`. With it the pane holds a
   * `CodeSession` open on the worktree — warming its code index, showing its progress in a strip
   * along the bottom, and answering definition, references and hover in the preview — and offers
   * the plan calls behind "Open as plan". Absent, the preview is read-only text and there is no
   * strip.
   */
  navigationClient?: Client<typeof CodeNavigationService>;
  /**
   * The session whose worktree this is. Unused since the strip follows the pane's `CodeSession`
   * rather than the session's `WatchCodeIndex`.
   *
   * TODO: remove, together with the `sessionId` that `SessionMainPane` and the Cypress pane
   * driver pass.
   */
  sessionId?: string;
};

/** `Index loading — <phase> <n>%`: what a query the loading index holds shows. */
function indexLoadingText(status: CodeIndexStatus): string {
  return status.kind === "progress"
    ? `Index loading — ${currentPhaseHeadline(status.progress)}`
    : "Index loading";
}

/**
 * Split Code pane: a lazy worktree directory tree on the left and a read-only file preview on the
 * right. Markdown renders as sanitized markup, everything else as monospace text. File content is
 * fetched on demand when a file node is selected. With a navigation client, a strip along the
 * bottom shows the worktree's code-index progress from the pane's `CodeSession`.
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
  const code = useCodeSession(navigationClient ?? null, { sessionToken, projectId, worktreePath });
  const plans = useMemo(
    () =>
      navigationClient
        ? createRestructurePlanApi(navigationClient, { sessionToken, projectId, worktreePath })
        : null,
    [navigationClient, sessionToken, projectId, worktreePath],
  );
  const nav = useCodeNavigation(api, code.session);
  const loadingText = indexLoadingText(code.status);
  const { selected, openFile } = nav;
  const handleSelectFile = useCallback((relPath: string) => openFile(relPath), [openFile]);

  // The index serves Rust only; offering ctrl-click and hover on other languages would ask it
  // questions it refuses.
  const navigable = code.session !== null && selected !== null && selected.relPath.endsWith(".rs");

  // A restructure plan opens as a plan only where the plan calls are served.
  const [openPlan, setOpenPlan] = useState<string | null>(null);
  const planFile =
    plans !== null &&
    selected !== null &&
    !selected.error &&
    isRestructurePlanFile(selected.relPath, selected.content)
      ? selected.relPath
      : null;

  return (
    <div
      data-testid="worktree-code-pane"
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
    >
      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
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
            {planFile !== null && (
              <div className="mb-2 flex justify-end">
                <button
                  type="button"
                  data-testid="worktree-code-open-as-plan"
                  className="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
                  onClick={() => setOpenPlan(planFile)}
                >
                  Open as plan
                </button>
              </div>
            )}
            <FilePreview
              selected={selected}
              onNavigate={navigable ? nav.navigate : undefined}
              onHover={navigable ? nav.dwell : undefined}
              isDefinable={navigable ? nav.isDefinable : undefined}
            />
          </section>
          {nav.hover && (
            <HoverCard
              hover={nav.hover}
              loadingText={loadingText}
              onShowReferences={nav.showReferences}
              onClose={nav.closeHover}
            />
          )}
          <div className="pointer-events-none absolute inset-x-3 bottom-3 flex flex-col gap-2">
            {nav.notice && <NavigationNotice notice={nav.notice} loadingText={loadingText} />}
            {nav.locations && <LocationListCard list={nav.locations} onOpen={nav.openLocation} />}
          </div>
        </div>
      </div>
      {navigationClient && <CodeIndexStatusStrip status={code.status} />}
      {plans !== null && openPlan !== null && (
        <RestructurePlanDialog api={plans} relPath={openPlan} onClose={() => setOpenPlan(null)} />
      )}
    </div>
  );
}

function FilePreview({
  selected,
  onNavigate,
  onHover,
  isDefinable,
}: {
  selected: SelectedFile | null;
  onNavigate?: (at: CodePosition) => void;
  onHover?: (at: CodePosition, anchor: HTMLElement) => void;
  isDefinable?: (at: CodePosition) => boolean;
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
      isDefinable={isDefinable}
      focusLine={selected.focusLine}
    />
  );
}
