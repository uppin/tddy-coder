/**
 * Page object for the session terminal tab bar (Agent + bash terminals).
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)` in specs.
 */

import {
  byTestId,
  sessionsTerminalTab,
  sessionsTerminalTabClose,
  sessionsTerminalPane,
  sessionsChildTab,
  sessionsChildPane,
  TEST_IDS,
} from "../testIds";

export const sessionTerminalTabsPage = {
  /** The terminal tab strip at the top of the session runtime area. */
  tabs: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsTerminalTabs, { timeout: 10000, ...options }),

  /** The fixed, non-closable Agent tab. */
  agentTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsTerminalTabAgent, { timeout: 10000, ...options }),

  /** The "+" new-terminal button. */
  newTab: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsTerminalTabNew, { timeout: 10000, ...options }),

  /** A single bash terminal tab, keyed by terminal id. */
  tab: (terminalId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTerminalTab(terminalId), { timeout: 10000, ...options }),

  /** The ✕ close control on a bash terminal tab. */
  tabClose: (terminalId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTerminalTabClose(terminalId), { timeout: 10000, ...options }),

  /** The mounted terminal pane for one terminal id (Agent uses "main"). */
  pane: (terminalId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTerminalPane(terminalId), { timeout: 10000, ...options }),

  /** The ghostty terminal canvas inside a terminal pane — the focus/typing target. */
  paneTerminal: (terminalId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTerminalPane(terminalId), { timeout: 10000, ...options }).find(
      `[data-testid='${TEST_IDS.ghosttyTerminal}']`,
    ),

  /** A tab for a spawned child conversation, keyed by the child's session id. */
  childTab: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsChildTab(sessionId), { timeout: 10000, ...options }),

  /** All child-conversation tabs currently rendered (prefix match) — for "no children" assertions. */
  childTabs: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("[data-testid^='sessions-child-tab-']", { timeout: 10000, ...options }),

  /** The mounted runtime pane for a selected child conversation, keyed by the child's session id. */
  childPane: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsChildPane(sessionId), { timeout: 10000, ...options }),
};
