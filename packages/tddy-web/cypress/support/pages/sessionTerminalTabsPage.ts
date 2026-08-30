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
  sessionsTerminalPaneStack,
  sessionsChildTab,
  sessionsChildPane,
  sessionsAgentTab,
  sessionsAgentTabClose,
  sessionsAgentPane,
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

  /** The trailing ⛶ full-screen toggle — acts on whichever pane is active. */
  fullscreenToggle: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsTerminalFullscreen, { timeout: 10000, ...options }),

  /** The floating "exit full screen" control, drawn only while a pane stack holds fullscreen. */
  fullscreenExit: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.sessionsTerminalFullscreenExit, { timeout: 10000, ...options }),

  /** One session runtime's pane stack — the element handed to the Fullscreen API. */
  paneStack: (sessionId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsTerminalPaneStack(sessionId), { timeout: 10000, ...options }),

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

  /** A tab for an open conversation with an attached agent, keyed by the conversation id. */
  agentConversationTab: (conversationId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsAgentTab(conversationId), { timeout: 10000, ...options }),

  /** The ✕ on an agent conversation tab — closing it cancels the conversation. */
  agentConversationTabClose: (conversationId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsAgentTabClose(conversationId), { timeout: 10000, ...options }),

  /** All agent conversation tabs currently rendered — for "exactly one" / "none" assertions. The
   *  close controls share the prefix, so they are excluded rather than counted as tabs. */
  agentConversationTabs: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("[data-testid^='sessions-agent-tab-']:not([data-testid$='-close'])", {
      timeout: 10000,
      ...options,
    }),

  /** The mounted body of a selected agent conversation, keyed by the conversation id. */
  agentConversationPane: (conversationId: string, options?: Parameters<typeof cy.get>[1]) =>
    byTestId(sessionsAgentPane(conversationId), { timeout: 10000, ...options }),
};
