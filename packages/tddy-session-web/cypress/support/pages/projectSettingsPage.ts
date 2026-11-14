/**
 * Page object for a project's settings pane (`#orchestrators` 11/11): the gear on a project header,
 * the pane it opens in the detail area — main's section and the orchestrator defaults — and where
 * the screen is.
 *
 * All raw selectors and location matching live here; test bodies call named methods. Main's own
 * form, its start and the `EnsureProjectHomeSession` calls are `projectHomeSetupPage`'s.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 */

import {
  byTestId,
  sessionsDrawerProjectHomeReplace,
  sessionsDrawerProjectSettings,
  sessionsProjectHomeSetup,
  sessionsProjectHomeSetupReplacing,
  sessionsProjectSettings,
  sessionsProjectSettingsDefaults,
  sessionsProjectSettingsMain,
  sessionsProjectSettingsReplaceMain,
} from "../testIds";

/** How long a lookup waits for something the screen renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

/** `#/…` with nothing after the path but an optional query. */
const hashIs = (path: string) => new RegExp(`^#${path}(\\?|$)`);

export const projectSettingsPage = {
  // ---------------------------------------------------------------------------
  // Drawer
  // ---------------------------------------------------------------------------

  gear: (projectId: string) => byTestId(sessionsDrawerProjectSettings(projectId), RENDER_WAIT),

  /** Presses the gear on `projectId`'s header. */
  openSettingsFor(projectId: string) {
    projectSettingsPage.gear(projectId).click();
  },

  /** Asserts `projectId`'s pinned main entry offers no Replace main of its own. */
  assertPinnedEntryOffersNoReplace(projectId: string) {
    byTestId(sessionsDrawerProjectHomeReplace(projectId)).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Settings pane
  // ---------------------------------------------------------------------------

  pane: (projectId: string) => byTestId(sessionsProjectSettings(projectId), RENDER_WAIT),

  mainSection: (projectId: string) => byTestId(sessionsProjectSettingsMain(projectId), RENDER_WAIT),

  defaultsSection: (projectId: string) => byTestId(sessionsProjectSettingsDefaults(projectId), RENDER_WAIT),

  replaceMain: (projectId: string) => byTestId(sessionsProjectSettingsReplaceMain(projectId), RENDER_WAIT),

  clickReplaceMain(projectId: string) {
    projectSettingsPage.replaceMain(projectId).click();
  },

  /** Main's setup form, inside the settings pane's main section. */
  mainForm: (projectId: string) =>
    projectSettingsPage
      .mainSection(projectId)
      .find(`[data-testid='${sessionsProjectHomeSetup(projectId)}']`, RENDER_WAIT),

  /** The notice main's form shows when starting replaces a main already set up. */
  replacingNotice: (projectId: string) =>
    projectSettingsPage
      .mainSection(projectId)
      .find(`[data-testid='${sessionsProjectHomeSetupReplacing(projectId)}']`, RENDER_WAIT),

  /**
   * Asserts the main section shows main's setup form — not main's summary — and offers no Replace
   * main: what a host with no main for the project shows. Waits for the form before asserting the
   * action absent, so the absence is not read off a pane still mounting.
   */
  assertMainIsNotSetUp(projectId: string) {
    projectSettingsPage.mainForm(projectId).should("contain", "Set up main");
    byTestId(sessionsProjectSettingsReplaceMain(projectId)).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Where the screen is
  // ---------------------------------------------------------------------------

  /** Asserts the location is `projectId`'s settings pane. */
  assertAtSettingsFor(projectId: string) {
    cy.location("hash").should("match", hashIs(`/sessions/project/${projectId}/settings`));
  },
};
