/**
 * Page object for the restructure plan dialog (`RestructurePlanDialog`) and the Code pane entry that
 * opens it (docs/ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md).
 *
 * All raw selectors live here; test bodies call named methods — no raw `cy.get(...)` in specs.
 */

import { byTestId, restructurePlanCell, restructurePlanRow, TEST_IDS } from "../testIds";

type GetOptions = Parameters<typeof cy.get>[1];

export const restructurePlanDialogPage = {
  /** The Code pane's "Open as plan" entry, shown while a plan file is previewed. */
  openAsPlan: (options?: GetOptions) =>
    byTestId(TEST_IDS.worktreeCodeOpenAsPlan, { timeout: 5000, ...options }),

  /** The dialog. */
  dialog: (options?: GetOptions) =>
    byTestId(TEST_IDS.restructurePlanDialog, { timeout: 5000, ...options }),

  /** The dialog's title: the plan file's path. */
  title: (options?: GetOptions) =>
    byTestId(TEST_IDS.restructurePlanTitle, { timeout: 5000, ...options }),

  /** One operation's row, keyed by its id. */
  row: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanRow(opId), { timeout: 5000, ...options }),

  /** The operation's kind cell. */
  op: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanCell(opId, "op"), { timeout: 5000, ...options }),

  /** The operation's item cell: the item path and the file it is in. */
  item: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanCell(opId, "item"), { timeout: 5000, ...options }),

  /** The operation's transactional group cell. */
  group: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanCell(opId, "group"), { timeout: 5000, ...options }),

  /** The operation's status cell. */
  status: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanCell(opId, "status"), {
      timeout: 5000,
      ...options,
    }),

  /** The operation's staleness cell. */
  stale: (opId: string, options?: GetOptions) =>
    byTestId(restructurePlanCell(opId, "stale"), { timeout: 5000, ...options }),

  /** The notice naming every stale operation and why. */
  staleNotice: (options?: GetOptions) =>
    byTestId(TEST_IDS.restructurePlanStaleNotice, {
      timeout: 5000,
      ...options,
    }),

  /** The Run button. */
  run: (options?: GetOptions) =>
    byTestId(TEST_IDS.restructurePlanRun, { timeout: 5000, ...options }),
};
