import { useEffect, useState } from "react";
import { ChevronDown, ChevronRight, File as FileIcon } from "lucide-react";

import type { WorktreeEntry, WorktreeFilesApi } from "./worktreeFilesApi";

/** `data-testid` for a single tree node, keyed by its worktree-root-relative path. */
const treeNodeTestId = (relPath: string) => `worktree-tree-node-${relPath}`;

function joinPath(parentPath: string, name: string): string {
  return parentPath ? `${parentPath}/${name}` : name;
}

/** Directories first, then files (each group keeps the server's alphabetical ordering). */
function directoriesFirst(entries: WorktreeEntry[]): WorktreeEntry[] {
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return 0;
  });
}

type TreeNodeProps = {
  entry: WorktreeEntry;
  parentPath: string;
  depth: number;
  api: WorktreeFilesApi;
  selectedRelPath: string | null;
  onSelectFile: (relPath: string) => void;
};

function TreeNode({ entry, parentPath, depth, api, selectedRelPath, onSelectFile }: TreeNodeProps) {
  const relPath = joinPath(parentPath, entry.name);
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<WorktreeEntry[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleClick = () => {
    if (entry.isDir) {
      const next = !expanded;
      setExpanded(next);
      if (next && children === null && !loading) {
        setLoading(true);
        setError(null);
        void api
          .listDir(relPath)
          .then((entries) => setChildren(directoriesFirst(entries)))
          // Leave `children` null so a later click retries; surface the failure instead of hanging.
          .catch((e: unknown) => setError(e instanceof Error ? e.message : "Failed to load folder"))
          .finally(() => setLoading(false));
      }
    } else {
      onSelectFile(relPath);
    }
  };

  const selected = !entry.isDir && selectedRelPath === relPath;

  return (
    <li>
      <button
        type="button"
        data-testid={treeNodeTestId(relPath)}
        onClick={handleClick}
        aria-expanded={entry.isDir ? expanded : undefined}
        data-selected={selected ? true : undefined}
        style={{ paddingLeft: `${depth * 12 + 4}px` }}
        className={`flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-sm hover:bg-muted ${
          selected ? "bg-muted font-medium" : ""
        }`}
      >
        {entry.isDir ? (
          expanded ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          )
        ) : (
          <FileIcon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        )}
        <span className="truncate">{entry.name}</span>
      </button>
      {entry.isDir && expanded && error && (
        <p
          style={{ paddingLeft: `${(depth + 1) * 12 + 4}px` }}
          className="py-0.5 text-xs text-destructive"
        >
          {error}
        </p>
      )}
      {entry.isDir && expanded && children && (
        <ul>
          {children.map((child) => (
            <TreeNode
              key={child.name}
              entry={child}
              parentPath={relPath}
              depth={depth + 1}
              api={api}
              selectedRelPath={selectedRelPath}
              onSelectFile={onSelectFile}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export type WorktreeFileTreeProps = {
  api: WorktreeFilesApi;
  selectedRelPath: string | null;
  onSelectFile: (relPath: string) => void;
};

/**
 * Recursive, lazy-expanding directory tree for a session worktree. The root level loads on mount;
 * each directory fetches its children only when it is first expanded. Selecting a file node calls
 * `onSelectFile` with the path relative to the worktree root.
 */
export function WorktreeFileTree({ api, selectedRelPath, onSelectFile }: WorktreeFileTreeProps) {
  const [root, setRoot] = useState<WorktreeEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    void api
      .listDir("")
      .then((entries) => {
        if (!cancelled) setRoot(directoriesFirst(entries));
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : "Failed to load worktree files");
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  return (
    <div
      data-testid="worktree-file-tree"
      className="h-full min-h-0 overflow-auto p-1"
      aria-label="Worktree files"
    >
      {error !== null ? (
        <p className="px-2 py-1 text-xs text-destructive">{error}</p>
      ) : root === null ? (
        <p className="px-2 py-1 text-xs text-muted-foreground">Loading files…</p>
      ) : (
        <ul>
          {root.map((entry) => (
            <TreeNode
              key={entry.name}
              entry={entry}
              parentPath=""
              depth={0}
              api={api}
              selectedRelPath={selectedRelPath}
              onSelectFile={onSelectFile}
            />
          ))}
        </ul>
      )}
    </div>
  );
}
