import React, { useState } from "react";

function logTddyMarker(markerId: string, scope: string): void {
  // Mirrors daemon development markers; visible in Cypress run logs.
  console.error(
    JSON.stringify({ tddy: { marker_id: markerId, scope, data: {} } }),
  );
}

export interface WorktreesScreenMockRow {
  path: string;
  branch: string;
  sizeLabel: string;
  changedFiles: number;
  linesAdded: number;
  linesRemoved: number;
  /** When true, stats may be outdated until Refresh is used. */
  stale?: boolean;
}

export interface WorktreesScreenProps {
  worktrees: WorktreesScreenMockRow[];
  onConfirmDelete?: (path: string) => void;
  /** Shown when the list is empty (e.g. pick Refresh stats). */
  emptyHint?: string;
}

/**
 * Project worktrees table (daemon-backed stats when wired to RPC).
 */
export function WorktreesScreen(props: WorktreesScreenProps) {
  const { worktrees, onConfirmDelete, emptyHint } = props;
  const [pendingDeletePath, setPendingDeletePath] = useState<string | null>(
    null,
  );

  logTddyMarker("M009", "tddy-web::WorktreesScreen");

  return (
    <div data-testid="worktrees-screen">
      <table data-testid="worktrees-table">
        <thead>
          <tr>
            <th scope="col">Path</th>
            <th scope="col">Branch</th>
            <th scope="col">Size</th>
            <th scope="col">Changed files</th>
            <th scope="col">+/- lines</th>
            <th scope="col">Actions</th>
          </tr>
        </thead>
        <tbody>
          {worktrees.length === 0 && emptyHint ? (
            <tr>
              <td colSpan={6} className="text-muted-foreground">
                {emptyHint}
              </td>
            </tr>
          ) : null}
          {worktrees.map((row) => (
            <tr key={row.path} data-testid="worktrees-row">
              <td>{row.path}</td>
              <td>
                {row.branch}
                {row.stale ? (
                  <span className="ml-1 text-xs text-muted-foreground">(stale)</span>
                ) : null}
              </td>
              <td>{row.sizeLabel}</td>
              <td>{row.changedFiles}</td>
              <td>
                +{row.linesAdded} / -{row.linesRemoved}
              </td>
              <td>
                <button
                  type="button"
                  data-testid="worktrees-delete"
                  onClick={() => {
                    setPendingDeletePath(row.path);
                  }}
                >
                  Delete
                </button>
                {pendingDeletePath === row.path ? (
                  <button
                    type="button"
                    data-testid="worktrees-delete-confirm"
                    onClick={() => {
                      onConfirmDelete?.(row.path);
                      setPendingDeletePath(null);
                    }}
                  >
                    Confirm delete
                  </button>
                ) : null}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
