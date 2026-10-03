import { useCallback, useMemo, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";

import type { CodeNavigationService } from "../../gen/code_navigation_pb";
import type { WorktreeService } from "../../gen/worktree_pb";
import { WorktreeFileTree } from "./WorktreeFileTree";
import { createWorktreeFilesApi } from "./worktreeFilesApi";
import { workflowPreviewKind } from "./sessionWorkflowPreview";
import { renderSimpleMarkdown } from "./renderSimpleMarkdown";
import { CodeBlock } from "./CodeBlock";
import {
  createCodeNavigationApi,
  type CodeLocationRef,
  type CodePosition,
} from "./codeNavigationApi";

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

type SelectedFile = {
  relPath: string;
  content: string;
  error: boolean;
  /** One-based line the preview scrolls to and marks as the navigation target. */
  focusLine?: number;
};

/** The hover card: the markdown the language server returned for the identifier at `at`. */
type Hover = { at: CodePosition; relPath: string; markdown: string };

/** A list of locations the user picks from — the references of a symbol, or ambiguous definitions. */
type LocationList = { title: string; locations: CodeLocationRef[] };

const errorMessage = (e: unknown, fallback: string) => (e instanceof Error ? e.message : fallback);


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

  const [selected, setSelected] = useState<SelectedFile | null>(null);
  const [hover, setHover] = useState<Hover | null>(null);
  const [locations, setLocations] = useState<LocationList | null>(null);
  const [notice, setNotice] = useState<{ text: string; isError: boolean } | null>(null);
  // Only the latest hover request may show its card: an older answer arriving late is dropped.
  const hoverRequest = useRef(0);

  const clearNavigationUi = useCallback(() => {
    hoverRequest.current += 1;
    setHover(null);
    setLocations(null);
    setNotice(null);
  }, []);

  const openFile = useCallback(
    (relPath: string, focusLine?: number) => {
      clearNavigationUi();
      void api
        .readFile(relPath)
        .then((res) => setSelected({ relPath, content: res.contentUtf8, error: false, focusLine }))
        .catch((e: unknown) => {
          setSelected({ relPath, content: errorMessage(e, "Failed to read file"), error: true });
        });
    },
    [api, clearNavigationUi],
  );

  const handleSelectFile = useCallback((relPath: string) => openFile(relPath), [openFile]);

  const openLocation = useCallback(
    (location: CodeLocationRef) => {
      if (location.outsideWorktree) return;
      openFile(location.relPath, location.start.line);
    },
    [openFile],
  );

  const showNavigationError = (what: string) => (e: unknown) =>
    setNotice({ text: `${what} failed: ${errorMessage(e, "unknown error")}`, isError: true });

  const selectedPath = selected && !selected.error ? selected.relPath : null;

  const handleNavigate = useCallback(
    (at: CodePosition) => {
      if (!navigation || selectedPath === null) return;
      setNotice(null);
      void navigation
        .definition(selectedPath, at)
        .then((found) => {
          if (found.length === 1) {
            openLocation(found[0]);
          } else if (found.length === 0) {
            setNotice({ text: "No definition found", isError: false });
          } else {
            setLocations({ title: "Definitions", locations: found });
          }
        })
        .catch(showNavigationError("Go to definition"));
    },
    [navigation, selectedPath, openLocation],
  );

  const handleHover = useCallback(
    (at: CodePosition) => {
      if (!navigation || selectedPath === null) return;
      const request = ++hoverRequest.current;
      void navigation
        .hover(selectedPath, at)
        .then((markdown) => {
          if (request !== hoverRequest.current) return;
          setHover(markdown === null ? null : { at, relPath: selectedPath, markdown });
        })
        .catch((e: unknown) => {
          if (request === hoverRequest.current) showNavigationError("Hover")(e);
        });
    },
    [navigation, selectedPath],
  );

  const handleShowReferences = useCallback(() => {
    if (!navigation || hover === null) return;
    setNotice(null);
    void navigation
      .references(hover.relPath, hover.at)
      .then((found) => setLocations({ title: "References", locations: found }))
      .catch(showNavigationError("Find references"));
  }, [navigation, hover]);

  // The index serves Rust only; offering ctrl-click and hover on other languages would ask it
  // questions it refuses.
  const navigable = navigation !== null && selected !== null && selected.relPath.endsWith(".rs");
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
      <div className="relative flex min-h-0 min-w-0 flex-1">
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
            <CodeBlock
              content={selected.content}
              relPath={selected.relPath}
              onNavigate={navigable ? handleNavigate : undefined}
              onHover={navigable ? handleHover : undefined}
              focusLine={selected.focusLine}
            />
          )}
        </section>
        <div className="pointer-events-none absolute inset-x-3 bottom-3 flex flex-col gap-2">
          {notice && (
            <p
              role={notice.isError ? "alert" : "status"}
              className={`pointer-events-auto rounded border border-border bg-background p-2 text-sm ${
                notice.isError ? "text-destructive" : "text-muted-foreground"
              }`}
            >
              {notice.text}
            </p>
          )}
          {locations && <LocationListCard list={locations} onOpen={openLocation} />}
          {hover && (
            <div
              data-testid="worktree-code-hover"
              className="pointer-events-auto rounded border border-border bg-background p-2 shadow"
            >
              <pre className="whitespace-pre-wrap font-mono text-sm">{hover.markdown}</pre>
              <div className="mt-2 flex gap-2">
                <button
                  type="button"
                  data-testid="worktree-code-references-action"
                  className="text-sm underline"
                  onClick={handleShowReferences}
                >
                  References
                </button>
                <button type="button" className="text-sm underline" onClick={() => setHover(null)}>
                  Close
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function LocationListCard({
  list,
  onOpen,
}: {
  list: LocationList;
  onOpen: (location: CodeLocationRef) => void;
}) {
  return (
    <div
      data-testid="worktree-code-references"
      className="pointer-events-auto max-h-48 overflow-auto rounded border border-border bg-background p-2 shadow"
    >
      <p className="text-sm font-medium">{list.title}</p>
      {list.locations.length === 0 && (
        <p className="text-sm text-muted-foreground">None found</p>
      )}
      <ul>
        {list.locations.map((location) => (
          <li key={`${location.relPath}:${location.start.line}:${location.start.column}`}>
            <button
              type="button"
              data-testid={`worktree-code-reference-${location.relPath}-${location.start.line}`}
              disabled={location.outsideWorktree}
              className="text-left font-mono text-sm underline disabled:cursor-not-allowed disabled:no-underline disabled:opacity-60"
              onClick={() => onOpen(location)}
            >
              {location.relPath}:{location.start.line}
              {location.outsideWorktree ? " (outside the worktree)" : ""}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
