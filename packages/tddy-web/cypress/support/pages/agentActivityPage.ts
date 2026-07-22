/**
 * Page object for the Agent Activity pane acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)`
 * in test files — only these named helpers. Mirrors `agentChatPage`'s style over the
 * `agent-activity-*` ids.
 */

import { byTestId, agentActivityRow, TEST_IDS } from "../testIds";

export const agentActivityPage = {
  /** The top-bar activity icon button (present only when the session has ≥1 tool-call record). */
  button: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityButton, { timeout: 5000, ...options }),

  /** The unread-activity badge on the icon. */
  unreadBadge: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityUnreadBadge, { timeout: 5000, ...options }),

  /** The in-pane activity overlay. */
  overlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityOverlay, { timeout: 5000, ...options }),

  /** The scrollable record list inside the overlay. */
  list: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityList, { timeout: 5000, ...options }),

  /** A single record row, addressed by its `call_id`. */
  row: (callId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentActivityRow(callId), { timeout: 5000, ...options }),

  /** The full-input/output detail dialog. */
  detailDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailDialog, { timeout: 5000, ...options }),

  /** The full tool input shown in the detail dialog. */
  detailInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailInput, { timeout: 5000, ...options }),

  /** The full tool output shown in the detail dialog. */
  detailOutput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailOutput, { timeout: 5000, ...options }),

  /** Open the activity overlay by clicking the top-bar icon. */
  open() {
    byTestId(TEST_IDS.agentActivityButton).click();
  },

  /** Close the activity overlay via its close control. */
  close() {
    byTestId(TEST_IDS.agentActivityOverlayClose).click();
  },

  /** Open the detail dialog for a record row addressed by its `call_id`. */
  openDetail(callId: string) {
    byTestId(agentActivityRow(callId)).click();
  },

  /** Close the detail dialog via its close control. */
  closeDetail() {
    byTestId(TEST_IDS.agentActivityDetailClose).click();
  },
};
