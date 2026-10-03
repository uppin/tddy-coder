/**
 * The restructure plan dialog, opened from the session Code pane's "Open as plan" entry on a plan
 * file: the plan's operations — id, kind, item, file, group, status — whether each still points at
 * the code it was written against, and a Run that applies the plan through the warm index with
 * per-operation progress.
 *
 * A stale operation disables Run and the dialog names it, with the plan store's reason.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md
 */

import React from "react";
import { Button } from "../ui/button";
import type { PlanOperationRow, RestructurePlanApi } from "./restructurePlanApi";

export interface RestructurePlanDialogProps {
  api: RestructurePlanApi;
  /** The plan file, relative to the worktree. */
  relPath: string;
  onClose: () => void;
}

/** The columns of the operation table, in order — also the suffix of each cell's test id. */
export const PLAN_COLUMNS = ["id", "op", "item", "group", "status", "stale"] as const;
export type PlanColumn = (typeof PLAN_COLUMNS)[number];

export function RestructurePlanDialog({ api: _api, relPath, onClose }: RestructurePlanDialogProps) {
  // TODO(plan-dialog): open the plan through `api.open`, follow `api.watch` for status and
  // staleness, and on Run follow `api.run`, turning each row applied as its event arrives and a
  // failed group's rows rolled back. Run is disabled while any operation is stale, and the stale
  // notice names each one with its reason.
  const rows: PlanOperationRow[] = [];
  const stale = rows.filter((row) => row.staleReason !== null);

  return (
    <div
      data-testid="restructure-plan-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={`Restructure plan ${relPath}`}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
    >
      <div className="bg-card text-card-foreground border-border flex max-h-full w-full max-w-3xl flex-col gap-4 rounded-xl border p-4 shadow-lg">
        <h2 data-testid="restructure-plan-title" className="text-sm font-semibold">
          {relPath}
        </h2>

        <div className="min-h-0 overflow-auto">
          <table className="w-full text-left text-xs">
            <thead>
              <tr>
                {PLAN_COLUMNS.map((column) => (
                  <th key={column} className="px-2 py-1 font-medium">
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <PlanRow key={row.id} row={row} />
              ))}
            </tbody>
          </table>
        </div>

        {stale.length > 0 && (
          <p data-testid="restructure-plan-stale-notice" className="text-destructive text-sm">
            {stale.map((row) => `${row.id}: stale — ${row.staleReason}`).join("; ")}
          </p>
        )}

        <div className="flex justify-end gap-2">
          <Button
            type="button"
            variant="secondary"
            data-testid="restructure-plan-close"
            onClick={onClose}
          >
            Close
          </Button>
          <Button type="button" data-testid="restructure-plan-run" disabled>
            Run
          </Button>
        </div>
      </div>
    </div>
  );
}

function PlanRow({ row }: { row: PlanOperationRow }) {
  const cells: Record<PlanColumn, string> = {
    id: row.id,
    op: row.op,
    item: row.item === "" ? row.file : `${row.item} (${row.file})`,
    group: row.group,
    status: row.status,
    stale: row.staleReason === null ? "" : `stale — ${row.staleReason}`,
  };
  return (
    <tr data-testid={`restructure-plan-row-${row.id}`} data-status={row.status}>
      {PLAN_COLUMNS.map((column) => (
        <td
          key={column}
          data-testid={`restructure-plan-row-${row.id}-${column}`}
          className="px-2 py-1"
        >
          {cells[column]}
        </td>
      ))}
    </tr>
  );
}
