/**
 * Page object for ConnectionScreen / AppRouting acceptance tests.
 *
 * All raw selectors live here; test bodies call named methods.
 */

import {
  byTestId,
  sessionsTable as sessionsTableId,
  connectBtn,
  deleteSessionBtn,
  signalDropdown as signalDropdownId,
  signalMenu,
  signalSigint,
  signalSigterm,
  signalSigkill,
  sessionRowSelect,
  sessionTableSelectAll,
  bulkDeleteButton,
  backendSelect,
  hostSelect,
  startSession,
  attachedTerminal,
  TEST_IDS,
} from "../testIds";

// ---------------------------------------------------------------------------
// Session table
// ---------------------------------------------------------------------------

export const connectionPage = {
  /** Wait for a project session table to appear. */
  sessionsTable: (projectId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTableId(projectId), { timeout: 5000, ...options }),

  /** The orphan "Other sessions" table. */
  orphanTable: () => byTestId(TEST_IDS.sessionsTableOrphan, { timeout: 5000 }),

  /** Connect button for a given session. */
  connectBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(connectBtn(sessionId), { timeout: 5000, ...options }),

  /** Delete button for a given session. */
  deleteSessionBtn: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(deleteSessionBtn(sessionId), options),

  // ---------------------------------------------------------------------------
  // Signal dropdown
  // ---------------------------------------------------------------------------

  /** The signal dropdown trigger button for a session. */
  signalDropdown: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(signalDropdownId(sessionId), { timeout: 5000, ...options }),

  /** The open signal dropdown menu for a session. */
  signalMenu: (sessionId: string) => byTestId(signalMenu(sessionId)),

  signalSigint: (sessionId: string) => byTestId(signalSigint(sessionId)),
  signalSigterm: (sessionId: string) => byTestId(signalSigterm(sessionId)),
  signalSigkill: (sessionId: string) => byTestId(signalSigkill(sessionId)),

  // ---------------------------------------------------------------------------
  // Bulk selection / delete
  // ---------------------------------------------------------------------------

  selectAll: (projectId: string) =>
    byTestId(sessionTableSelectAll(projectId), { timeout: 5000 }),

  sessionRowCheckbox: (sessionId: string) => byTestId(sessionRowSelect(sessionId)),

  bulkDeleteButton: (projectId: string) => byTestId(bulkDeleteButton(projectId)),

  // ---------------------------------------------------------------------------
  // Start session
  // ---------------------------------------------------------------------------

  startSession: (projectId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(startSession(projectId), { timeout: 8000, ...options }),

  // ---------------------------------------------------------------------------
  // Backend / host selects
  // ---------------------------------------------------------------------------

  backendSelect: (projectId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(backendSelect(projectId), { timeout: 8000, ...options }),

  hostSelect: (rowKey: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(hostSelect(rowKey), { timeout: 5000, ...options }),

  // ---------------------------------------------------------------------------
  // Terminal chrome (after connecting)
  // ---------------------------------------------------------------------------

  /** The terminal container that holds the GhosttyTerminal after a connect. */
  terminalContainer: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectedTerminalContainer, { timeout: 5000, ...options }),

  /** The overlay root shown for a session reconnect. */
  reconnectOverlay: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalReconnectOverlayRoot, { timeout: 15000, ...options }),

  /** The "Expand" link inside the reconnect overlay. */
  reconnectExpand: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalReconnectExpand, { timeout: 15000, ...options }),

  /** The terminal connection status dot. */
  statusDot: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectionStatusDot, { timeout: 20000, ...options }),

  /** "Disconnect" item inside the status dot menu. */
  disconnectMenuItem: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectionMenuDisconnect, { timeout: 10000, ...options }),

  /** "Terminate" item inside the status dot menu. */
  terminateMenuItem: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectionMenuTerminate, { timeout: 10000, ...options }),

  /** LiveKit status indicator (hidden while connected). */
  livekitStatus: () => byTestId(TEST_IDS.livekitStatus),

  /** Inline error message shown when an RPC fails. */
  connectionError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.connectionError, { timeout: 5000, ...options }),

  /** The terminal route "unknown session" error panel. */
  unknownSessionError: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.terminalRouteUnknownSession, { timeout: 8000, ...options }),

  unknownSessionHomeLink: () => byTestId(TEST_IDS.terminalRouteUnknownSessionHome),

  /** `[data-testid="connection-attached-terminal-<sessionId>"]` */
  attachedTerminal: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(attachedTerminal(sessionId), { timeout: 15000, ...options }),
};
