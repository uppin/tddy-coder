/**
 * Page object for the orchestrator composer (`#orchestrators` 8/11): its pane, the prompt box, the
 * controls under the box, and the `StartSession` calls the daemon double recorded.
 *
 * All raw selectors, geometry and wire inspection live here; test bodies call named methods.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { TEST_IDS, byTestId, sessionsProjectComposer } from "../testIds";

/** How long a lookup waits for something the composer renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

/** A daemon double recording the unary calls it answered. */
type RecordingBackend = Pick<InMemoryRpcBackend, "callsTo">;

/** The vertical centre of an element's box, in viewport pixels. */
function verticalCentre(element: HTMLElement): number {
  const rect = element.getBoundingClientRect();
  return rect.top + rect.height / 2;
}

export const orchestratorComposerPage = {
  pane: (projectId: string) => byTestId(sessionsProjectComposer(projectId), RENDER_WAIT),

  box: () => byTestId(TEST_IDS.sessionsProjectComposerInput, RENDER_WAIT),

  sendButton: () => byTestId(TEST_IDS.sessionsProjectComposerSend, RENDER_WAIT),

  write(text: string) {
    orchestratorComposerPage.box().type(text);
  },

  /** Presses Enter in the box. */
  pressEnter() {
    orchestratorComposerPage.box().type("{enter}");
  },

  send() {
    orchestratorComposerPage.sendButton().should("not.be.disabled").click();
  },

  chooseHost(instanceId: string) {
    byTestId(TEST_IDS.createSessionHostSelect, RENDER_WAIT).select(instanceId);
  },

  /** The test ids of the `<select>` controls the composer shows, in document order. */
  selectTestIds: (projectId: string): Cypress.Chainable<string[]> =>
    orchestratorComposerPage
      .pane(projectId)
      // Wait for the served controls before reading the set, so it is not read off a mounting pane.
      .find(`[data-testid='${TEST_IDS.createSessionModelSelect}']`, RENDER_WAIT)
      .then(() =>
        orchestratorComposerPage
          .pane(projectId)
          .find("select")
          .then(($selects) => [...$selects].map((s) => s.getAttribute("data-testid") ?? "")),
      ),

  /** The composer's checkboxes — the New-session form's toggles, none of which it offers. */
  checkboxes: (projectId: string) => orchestratorComposerPage.pane(projectId).find("input[type='checkbox']"),

  attachments: (projectId: string) =>
    orchestratorComposerPage.pane(projectId).find(`[data-testid='${TEST_IDS.createSessionAttachmentsSection}']`),

  /** Asserts the composer fills the `height`-pixel pane it is mounted in. */
  assertFillsPaneOf(projectId: string, height: number) {
    orchestratorComposerPage.pane(projectId).should(($pane) => {
      expect($pane[0].getBoundingClientRect().height, "composer height").to.be.closeTo(height, 1);
    });
  },

  /**
   * Asserts the prompt box's vertical centre is within `tolerance` (a fraction of the composer's
   * height) of the composer's own: the heading above and the controls under it shift it a little,
   * a box at the top or the bottom of the pane is far off.
   */
  assertBoxVerticallyCentredIn(projectId: string, tolerance: number) {
    orchestratorComposerPage.pane(projectId).then(($pane) => {
      const pane = $pane[0];
      orchestratorComposerPage.box().should(($box) => {
        const offset = Math.abs(verticalCentre($box[0]) - verticalCentre(pane));
        expect(offset, "box centre's distance from the pane's").to.be.lessThan(
          pane.getBoundingClientRect().height * tolerance,
        );
      });
    });
  },

  /** The one `StartSession` request the composer sent. */
  theStartRequest: (backend: RecordingBackend) =>
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(SessionService.method.startSession), "one StartSession request").to.have.length(1);
    }).then((b) => b.callsTo(SessionService.method.startSession)[0]),

  assertNothingStarted(backend: RecordingBackend) {
    cy.wrap(backend).should((b) => {
      expect(b.callsTo(SessionService.method.startSession), "StartSession requests").to.have.length(0);
    });
  },
};
