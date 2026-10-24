/**
 * Page object for a project's **main** setup: the pinned entry in the sessions drawer, the setup
 * pane (the New-session form in its main variant), the Projects card link to it, and the
 * `EnsureProjectHomeSession` calls the daemon double recorded.
 *
 * All raw selectors and wire inspection live here; test bodies call named methods.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  TEST_IDS,
  byTestId,
  createSessionTypeReason,
  projectMainSessionLink,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectHome,
  sessionsDrawerProjectHomeReplace,
  sessionsProjectHomeSetup,
  sessionsProjectHomeSetupReplacing,
  sessionsProjectHomeSetupUnknown,
  sessionsProjectHomeSetupUnreachable,
} from "../testIds";
import type { SessionType } from "../../../src/components/sessions/createSessionRequest";
import { SessionService } from "../../../src/gen/session_pb";

/** How long a lookup waits for something the screen renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

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

/** Any pinned **main** entry's own test id prefix (its Replace action and error share it). */
const ANY_PINNED_ENTRY = "[data-testid^='sessions-drawer-project-home-']";

/** Any project group header, in the drawer's order. */
const ANY_GROUP_HEADER = "[data-testid^='sessions-drawer-project-group-header-']";

/** `#/…` with nothing after the path but an optional query. */
const hashIs = (path: string) => new RegExp(`^#${path}(\\?|$)`);

/** A daemon double recording the unary calls it answered. */
type RecordingBackend = Pick<InMemoryRpcBackend, "callsTo">;

/** What one recorded `EnsureProjectHomeSession` must have asked for. */
interface ExpectedEnsure {
  projectId: string;
  /** Fields the start spec must carry; `null`: the ensure carried no start spec. */
  start: Record<string, unknown> | null;
}

export const projectHomeSetupPage = {
  // ---------------------------------------------------------------------------
  // Drawer entry
  // ---------------------------------------------------------------------------

  mainEntry: (projectId: string) => byTestId(sessionsDrawerProjectHome(projectId), RENDER_WAIT),

  clickMain(projectId: string) {
    projectHomeSetupPage.mainEntry(projectId).click();
  },

  replaceAction: (projectId: string) => byTestId(sessionsDrawerProjectHomeReplace(projectId), RENDER_WAIT),

  clickReplaceMain(projectId: string) {
    projectHomeSetupPage.replaceAction(projectId).click();
  },

  /** Asserts the group for `groupKey` (a project id, or `other`) has no pinned **main** entry. */
  assertNoPinnedEntryIn(groupKey: string) {
    byTestId(sessionsDrawerProjectGroup(groupKey)).find(ANY_PINNED_ENTRY).should("not.exist");
  },

  /** The project group headers' test ids, in the drawer's order. */
  groupHeaderOrder: (): Cypress.Chainable<string[]> =>
    cy.get(ANY_GROUP_HEADER).then(($headers) => [...$headers].map((h) => h.getAttribute("data-testid")!)),

  // ---------------------------------------------------------------------------
  // Setup pane
  // ---------------------------------------------------------------------------

  pane: (projectId: string) => byTestId(sessionsProjectHomeSetup(projectId), RENDER_WAIT),

  /** The New-session form inside the setup pane. */
  form: (projectId: string) =>
    projectHomeSetupPage.pane(projectId).find(`[data-testid="${TEST_IDS.createSessionPane}"]`),

  replacingNotice: (projectId: string) => byTestId(sessionsProjectHomeSetupReplacing(projectId)),

  unknownProject: (projectId: string) => byTestId(sessionsProjectHomeSetupUnknown(projectId)),

  unreachableHost: (projectId: string) => byTestId(sessionsProjectHomeSetupUnreachable(projectId)),

  typeButton: (sessionType: SessionType) => byTestId(TYPE_BUTTON[sessionType]),

  typeReason: (sessionType: SessionType) => byTestId(createSessionTypeReason(sessionType)),

  /**
   * Asserts the main variant renders none of the controls it hides. Waits for the form to have
   * rendered — its model select visible — before asserting each one absent, so an absence is
   * not read off a form still mounting.
   */
  assertMainVariantHidesPlacementAndBranchControls(projectId: string) {
    projectHomeSetupPage
      .form(projectId)
      .find(`[data-testid="${TEST_IDS.createSessionModelSelect}"]`, RENDER_WAIT)
      .should("be.visible");
    for (const id of HIDDEN_IN_MAIN) {
      projectHomeSetupPage.pane(projectId).find(`[data-testid="${id}"]`).should("not.exist");
    }
  },

  chooseModel(model: string) {
    byTestId(TEST_IDS.createSessionModelSelect, RENDER_WAIT).select(model);
  },

  choosePermissionMode(mode: string) {
    byTestId(TEST_IDS.createSessionPermissionModeSelect).select(mode);
  },

  typeInitialPrompt(prompt: string) {
    byTestId(TEST_IDS.createSessionInitialPromptInput).type(prompt);
  },

  submitButton: () => byTestId(TEST_IDS.createSessionSubmitBtn),

  startMain() {
    projectHomeSetupPage.submitButton().should("not.be.disabled").click();
  },

  cancel() {
    byTestId(TEST_IDS.createSessionCancelBtn).click();
  },

  error: () => byTestId(TEST_IDS.createSessionError, RENDER_WAIT),

  // ---------------------------------------------------------------------------
  // Where the screen is
  // ---------------------------------------------------------------------------

  /** Asserts the location is `projectId`'s setup pane, in replace mode when `replacing`. */
  assertAtSetupFor(projectId: string, { replacing = false }: { replacing?: boolean } = {}) {
    const suffix = replacing ? "/replace" : "";
    cy.location("hash").should("match", hashIs(`/sessions/project-home/${projectId}${suffix}`));
  },

  /** Asserts the location selects `sessionId`. */
  assertSessionSelected(sessionId: string) {
    cy.location("hash").should("match", hashIs(`/sessions/${sessionId}`));
  },

  /** Asserts the location is the drawer root, nothing selected. */
  assertAtDrawerRoot() {
    cy.location("hash").should("match", hashIs("/sessions"));
  },

  // ---------------------------------------------------------------------------
  // What the daemon was asked
  // ---------------------------------------------------------------------------

  /** Asserts `backend` was sent exactly one `EnsureProjectHomeSession`, asking for `expected`. */
  assertEnsuredOnce(backend: RecordingBackend, expected: ExpectedEnsure) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(SessionService.method.ensureProjectHomeSession);
      expect(calls).to.have.length(1);
      expect(calls[0].projectId).to.equal(expected.projectId);
      if (expected.start === null) {
        expect(calls[0].start, "no start spec").to.equal(undefined);
      } else {
        expect(calls[0].start).to.include(expected.start);
      }
    });
  },

  // ---------------------------------------------------------------------------
  // Projects card
  // ---------------------------------------------------------------------------

  projectCardLink: (projectId: string) => byTestId(projectMainSessionLink(projectId), RENDER_WAIT),
};
