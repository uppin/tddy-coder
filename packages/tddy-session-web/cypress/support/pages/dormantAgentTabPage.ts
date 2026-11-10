/**
 * Page object for a **stopped session's Agent tab** — the greyed, read-only terminal of what the agent
 * last printed, with the toast that says the session is not running and offers Resume.
 *
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/session-terminal-tabs.md
 */

import { byTestId, TEST_IDS } from "../testIds";

type GetOptions = Parameters<typeof cy.get>[1];

export const dormantAgentTabPage = {
  /** The greyed terminal; carries `data-color-scheme="history"`. */
  terminal: (options?: GetOptions) =>
    byTestId(TEST_IDS.dormantAgentTerminal, { timeout: 10000, ...options }),

  /** "No recorded output" — shown when the daemon has no recording for the agent terminal. */
  empty: (options?: GetOptions) =>
    byTestId(TEST_IDS.dormantAgentTerminalEmpty, { timeout: 10000, ...options }),

  /** The toast: "This session is not running". */
  toast: (options?: GetOptions) =>
    byTestId(TEST_IDS.dormantAgentToast, { timeout: 10000, ...options }),

  /** The toast's Resume button. */
  toastResume: (options?: GetOptions) =>
    byTestId(TEST_IDS.dormantAgentToastAction, { timeout: 10000, ...options }),

  /** Why the last Resume from the toast failed. */
  toastError: (options?: GetOptions) =>
    byTestId(TEST_IDS.dormantAgentToastError, { timeout: 10000, ...options }),
};
