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
  sessionsVncTargetRow,
  sessionsVncStartBtn,
  sessionsVncStopBtn,
  sessionsVncRemoveBtn,
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

  // ---------------------------------------------------------------------------
  // VNC tab
  // ---------------------------------------------------------------------------

  /** The VNC tab button in the inspector tab strip. */
  inspectorVncTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsInspectorTabVnc, { timeout: 5000, ...options }),

  /** The VNC tab panel (rendered when the VNC tab is active). */
  vncTabPanel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncTabPanel, { timeout: 5000, ...options }),

  /** The list of VNC targets. */
  vncTargetList: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncTargetList, { timeout: 5000, ...options }),

  /** A single VNC target row. */
  vncTargetRow: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncTargetRow(targetId), { timeout: 5000, ...options }),

  /** The Start button for a given target. */
  vncStartBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncStartBtn(targetId), { timeout: 5000, ...options }),

  /** The Stop button for a given target. */
  vncStopBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncStopBtn(targetId), { timeout: 5000, ...options }),

  /** The Remove button for a given target. */
  vncRemoveBtn: (targetId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsVncRemoveBtn(targetId), { timeout: 5000, ...options }),

  /** The Add VNC target form. */
  vncAddForm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddForm, { timeout: 5000, ...options }),

  /** The label input in the Add form. */
  vncAddLabel: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddLabel, options),

  /** The host input in the Add form. */
  vncAddHost: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddHost, options),

  /** The port input in the Add form. */
  vncAddPort: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddPort, options),

  /** The password input in the Add form. */
  vncAddPassword: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddPassword, options),

  /** The submit button in the Add form. */
  vncAddSubmit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncAddSubmit, { timeout: 5000, ...options }),

  /** The passphrase dialog. */
  vncPassphraseDialog: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseDialog, { timeout: 5000, ...options }),

  /** The passphrase input in the dialog. */
  vncPassphraseInput: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseInput, options),

  /** The confirm button in the passphrase dialog. */
  vncPassphraseConfirm: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsVncPassphraseConfirm, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // VNC overlay
  // ---------------------------------------------------------------------------

  /** The full-screen VNC desktop overlay. */
  vncOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlay, { timeout: 5000, ...options }),

  /** The close button inside the VNC overlay. */
  vncOverlayClose: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlayClose, { timeout: 5000, ...options }),

  /** The `<video>` element inside the VNC overlay. */
  vncOverlayVideo: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.vncOverlayVideo, { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Terminal control mutex — "Claim terminal" CTA
  // ---------------------------------------------------------------------------

  /** The overlay that appears when this screen is not the terminal controller. */
  terminalControlOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlOverlay, { timeout: 5000, ...options }),

  /** The "Claim terminal" button inside the control overlay. */
  terminalClaimBtn: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalClaimBtn, { timeout: 5000, ...options }),

  /** The text element naming the screen currently holding control. */
  terminalControlHolder: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalControlHolder, { timeout: 5000, ...options }),
};
