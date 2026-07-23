/**
 * Page object for the unified AppShell top chrome (hamburger navigation menu).
 *
 * All raw selectors live here; test bodies call named methods. No raw `cy.get(...)`
 * in test files — only these named helpers.
 */

import { byTestId, TEST_IDS } from "../testIds";

export const appShellPage = {
  /** The top-left hamburger button that opens the navigation menu. */
  menuButton: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.shellMenuButton, { timeout: 5000, ...options }),

  /** Open the navigation menu (click the hamburger). */
  openMenu: () => {
    byTestId(TEST_IDS.shellMenuButton, { timeout: 5000 }).click();
  },

  /** The open menu container (role="menu"). */
  menu: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("[role='menu']", { timeout: 5000, ...options }),

  /** All menu item buttons in the open menu. */
  menuItems: (options?: Parameters<typeof cy.get>[1]) =>
    cy.get("[role='menu'] [role='menuitem']", { timeout: 5000, ...options }),

  /** The Sessions menu item. */
  sessionsItem: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.shellMenuSessions, { timeout: 5000, ...options }),

  /** The LiveKit menu item. */
  livekitItem: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.shellMenuLivekit, { timeout: 5000, ...options }),

  /** The Worktrees menu item. */
  worktreesItem: (options?: Parameters<typeof cy.get>[1]) =>
    byTestId(TEST_IDS.shellMenuWorktrees, { timeout: 5000, ...options }),
};
