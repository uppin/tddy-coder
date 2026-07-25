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

  /** A single record row, addressed by its `call_id` (legacy row list — used to assert absence). */
  row: (callId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(agentActivityRow(callId), { timeout: 5000, ...options }),

  /** Open the activity overlay by clicking the top-bar icon. */
  open() {
    byTestId(TEST_IDS.agentActivityButton).click();
  },

  /** Close the activity overlay via its close control. */
  close() {
    byTestId(TEST_IDS.agentActivityOverlayClose).click();
  },

  // ---------------------------------------------------------------------------
  // Tool-call detail dialog (prettified, color-highlighted JSON)
  // ---------------------------------------------------------------------------

  /** The tool-call detail dialog. */
  detailDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailDialog, { timeout: 5000, ...options }),

  /** The dialog's raw_input JSON block. */
  detailInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailInput, { timeout: 5000, ...options }),

  /** The dialog's raw_output JSON block. */
  detailOutput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityDetailOutput, { timeout: 5000, ...options }),

  /** Any color-highlighted JSON block (Prism output) inside the dialog. */
  jsonHighlight: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.agentActivityJsonHighlight, { timeout: 5000, ...options }),

  /** Open the detail dialog by clicking the transcript entry at `index` (arrival order). */
  openDetail(index: number) {
    byTestId(`agent-chat-message-${index}`).click();
  },

  /** Close the detail dialog via its close control. */
  closeDetail() {
    byTestId(TEST_IDS.agentActivityDetailClose).click();
  },
};
