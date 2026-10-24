/**
 * Page object for a project's **main** setup: the pinned entry in the sessions drawer, the setup
 * pane (the New-session form in its main variant) and the Projects card link to it.
 *
 * All raw selectors live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */

import {
  TEST_IDS,
  byTestId,
  createSessionTypeReason,
  projectMainSessionLink,
  sessionsDrawerProjectHome,
  sessionsDrawerProjectHomeReplace,
  sessionsProjectHomeSetup,
  sessionsProjectHomeSetupReplacing,
  sessionsProjectHomeSetupUnknown,
} from "../testIds";
import type { SessionType } from "../../../src/components/sessions/createSessionRequest";

/** The type-toggle button id for each session type. */
const TYPE_BUTTON: Record<SessionType, string> = {
  tool: TEST_IDS.createSessionTypeToolBtn,
  "claude-cli": TEST_IDS.createSessionTypeClaudeCliBtn,
  "cursor-cli": TEST_IDS.createSessionTypeCursorCliBtn,
  "opencode-cli": TEST_IDS.createSessionTypeOpenCodeCliBtn,
  "muse-cli": TEST_IDS.createSessionTypeMuseCliBtn,
};

/** Every control the main variant hides, by the New-session form's own test id. */
const HIDDEN_IN_MAIN = [
  TEST_IDS.createSessionHostSelect,
  TEST_IDS.createSessionProjectSelect,
  TEST_IDS.createSessionBranchIntentSelect,
  TEST_IDS.createSessionStackParentSelect,
  TEST_IDS.createSessionAttachmentsSection,
  TEST_IDS.createSessionSandboxToggle,
  TEST_IDS.createSessionManagedCodebaseToggle,
  TEST_IDS.createSessionSandboxedCodebaseToggle,
] as const;

export const projectHomeSetupPage = {
  // ---------------------------------------------------------------------------
  // Drawer entry
  // ---------------------------------------------------------------------------

  mainEntry: (projectId: string) => byTestId(sessionsDrawerProjectHome(projectId), { timeout: 5000 }),

  clickMain(projectId: string) {
    projectHomeSetupPage.mainEntry(projectId).click();
  },

  replaceAction: (projectId: string) => byTestId(sessionsDrawerProjectHomeReplace(projectId)),

  clickReplaceMain(projectId: string) {
    byTestId(sessionsDrawerProjectHomeReplace(projectId), { timeout: 5000 }).click();
  },

  // ---------------------------------------------------------------------------
  // Setup pane
  // ---------------------------------------------------------------------------

  pane: (projectId: string) => byTestId(sessionsProjectHomeSetup(projectId), { timeout: 5000 }),

  /** The New-session form inside the setup pane. */
  form: (projectId: string) =>
    projectHomeSetupPage.pane(projectId).find(`[data-testid="${TEST_IDS.createSessionPane}"]`),

  replacingNotice: (projectId: string) => byTestId(sessionsProjectHomeSetupReplacing(projectId)),

  unknownProject: (projectId: string) => byTestId(sessionsProjectHomeSetupUnknown(projectId)),

  typeButton: (sessionType: SessionType) => byTestId(TYPE_BUTTON[sessionType]),

  typeReason: (sessionType: SessionType) => byTestId(createSessionTypeReason(sessionType)),

  /** The test ids of the hidden controls that are nevertheless rendered (expected: none). */
  renderedHiddenControls: (projectId: string): Cypress.Chainable<string[]> =>
    projectHomeSetupPage
      .pane(projectId)
      .then(($pane) =>
        HIDDEN_IN_MAIN.filter((id) => $pane.find(`[data-testid="${id}"]`).length > 0),
      ),

  chooseModel(model: string) {
    byTestId(TEST_IDS.createSessionModelSelect, { timeout: 5000 }).select(model);
  },

  choosePermissionMode(mode: string) {
    byTestId(TEST_IDS.createSessionPermissionModeSelect).select(mode);
  },

  typeInitialPrompt(prompt: string) {
    byTestId(TEST_IDS.createSessionInitialPromptInput).type(prompt);
  },

  submitButton: () => byTestId(TEST_IDS.createSessionSubmitBtn),

  startMain() {
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();
  },

  cancel() {
    byTestId(TEST_IDS.createSessionCancelBtn).click();
  },

  error: () => byTestId(TEST_IDS.createSessionError, { timeout: 5000 }),

  // ---------------------------------------------------------------------------
  // Projects card
  // ---------------------------------------------------------------------------

  projectCardLink: (projectId: string) => byTestId(projectMainSessionLink(projectId), { timeout: 5000 }),
};
