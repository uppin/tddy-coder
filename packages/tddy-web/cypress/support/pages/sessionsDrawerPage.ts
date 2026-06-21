/**
 * Page object for the SessionsDrawerScreen acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 * No raw `cy.get(...)` in test files — only these named helpers.
 */

import {
  byTestId,
  sessionsDrawerItem,
  sessionsDrawerItemLabel,
  sessionsDrawerItemStatus,
  sessionsDrawerItemTooltip,
  sessionsDetailResumeBtn,
  sessionsDetailDeleteBtn,
  sessionsInspectorResumeBtn,
  sessionsInspectorDeleteBtn,
  sessionsInspectorDeleteConfirm,
  sessionsInspectorTerminateBtn,
  TEST_IDS,
} from "../testIds";

// ---------------------------------------------------------------------------
// Sessions drawer screen — page object
// ---------------------------------------------------------------------------

export const sessionsDrawerPage = {
  // ---------------------------------------------------------------------------
  // Screen root
  // ---------------------------------------------------------------------------

  /** The sessions drawer screen root. */
  screen: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawerScreen, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Drawer (left sidebar)
  // ---------------------------------------------------------------------------

  /** The scrollable drawer containing all session items. */
  drawer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDrawer, { timeout: 5000, ...options }),

  /** A single clickable drawer item for the given session id. */
  drawerItem: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItem(sessionId), { timeout: 5000, ...options }),

  /** The derived label text inside a drawer item. */
  drawerItemLabel: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemLabel(sessionId), options),

  /** The status indicator dot inside a drawer item. */
  drawerItemStatus: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemStatus(sessionId), options),

  /** The tooltip content element (visible on hover) that contains the full session id. */
  drawerItemTooltip: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDrawerItemTooltip(sessionId), options),

  // ---------------------------------------------------------------------------
  // Detail pane (right area)
  // ---------------------------------------------------------------------------

  /** The detail pane container (right of the drawer). */
  detailPane: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailPane, { timeout: 5000, ...options }),

  /** The terminal container rendered when a connected session is selected. */
  detailTerminalContainer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailTerminalContainer, { timeout: 10000, ...options }),

  /** The metadata block rendered when a disconnected session is selected. */
  detailMetadata: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsDetailMetadata, { timeout: 5000, ...options }),

  /** The Resume button rendered for a disconnected session. */
  detailResumeBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDetailResumeBtn(sessionId), { timeout: 5000, ...options }),

  /** The Delete button rendered for a disconnected session in the detail pane. */
  detailDeleteBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsDetailDeleteBtn(sessionId), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Inspector drawer (right overlay)
  // ---------------------------------------------------------------------------

  /** The inspector drawer element — check data-state attribute ("closed"|"open"|"expanded"). */
  inspectorDrawer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorDrawer, { timeout: 5000, ...options }),

  /** The toggle button that opens/closes the inspector. */
  inspectorToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorToggle, { timeout: 5000, ...options }),

  /** The close button inside the inspector header. */
  inspectorClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorClose, { timeout: 5000, ...options }),

  /** The expand button inside the inspector header. */
  inspectorExpand: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorExpand, { timeout: 5000, ...options }),

  /** The restore button inside the inspector header (visible only in expanded state). */
  inspectorRestore: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorRestore, { timeout: 5000, ...options }),

  /** The metadata section inside the inspector. */
  inspectorMetadata: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorMetadata, { timeout: 5000, ...options }),

  /** The Resume button inside the inspector for the given session. */
  inspectorResumeBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorResumeBtn(sessionId), { timeout: 5000, ...options }),

  /** The Delete button inside the inspector for the given session. */
  inspectorDeleteBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorDeleteBtn(sessionId), { timeout: 5000, ...options }),

  /** The confirm-delete button (second click) inside the inspector. */
  inspectorDeleteConfirm: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorDeleteConfirm(sessionId), { timeout: 5000, ...options }),

  /** The Terminate button inside the inspector for the given session. */
  inspectorTerminateBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsInspectorTerminateBtn(sessionId), { timeout: 5000, ...options }),
};
