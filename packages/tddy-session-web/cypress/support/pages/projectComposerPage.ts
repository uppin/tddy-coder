/**
 * Page object for starting an orchestrator from the drawer (`#orchestrators` 8/11): the "+" on a
 * project header, the composer it opens in the detail area, and the `StartSession` calls the daemon
 * double recorded.
 *
 * All raw selectors and wire inspection live here; test bodies call named methods.
 *
 * Feature: docs/ft/web/session-drawer.md § Orchestrator composer
 */

import type { InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionService } from "tddy-client-web/gen/session_pb";
import {
  TEST_IDS,
  byTestId,
  sessionsDrawerProjectGroupHeader,
  sessionsDrawerProjectNew,
  sessionsProjectComposer,
} from "../testIds";

/** How long a lookup waits for something the screen renders after an RPC answers. */
const RENDER_WAIT = { timeout: 5000 };

/** `#/…` with nothing after the path but an optional query. */
const hashIs = (path: string) => new RegExp(`^#${path}(\\?|$)`);

/** A daemon double recording the unary calls it answered. */
type RecordingBackend = Pick<InMemoryRpcBackend, "callsTo">;

export const projectComposerPage = {
  // ---------------------------------------------------------------------------
  // Drawer
  // ---------------------------------------------------------------------------

  groupHeader: (projectId: string) => byTestId(sessionsDrawerProjectGroupHeader(projectId), RENDER_WAIT),

  plus: (projectId: string) => byTestId(sessionsDrawerProjectNew(projectId), RENDER_WAIT),

  /** Presses the "+" on `projectId`'s header. */
  openComposerFor(projectId: string) {
    projectComposerPage.plus(projectId).click();
  },

  // ---------------------------------------------------------------------------
  // Composer
  // ---------------------------------------------------------------------------

  composer: (projectId: string) => byTestId(sessionsProjectComposer(projectId), RENDER_WAIT),

  box: () => byTestId(TEST_IDS.sessionsProjectComposerInput, RENDER_WAIT),

  error: () => byTestId(TEST_IDS.sessionsProjectComposerError, RENDER_WAIT),

  write(text: string) {
    projectComposerPage.box().type(text);
  },

  /** Picks an effort level from the composer's effort control (the New-session form's own). */
  chooseEffort(level: string) {
    byTestId(TEST_IDS.createSessionEffortSelect, RENDER_WAIT).select(level);
  },

  send() {
    byTestId(TEST_IDS.sessionsProjectComposerSend).should("not.be.disabled").click();
  },

  // ---------------------------------------------------------------------------
  // Where the screen is
  // ---------------------------------------------------------------------------

  assertAtComposerFor(projectId: string) {
    cy.location("hash").should("match", hashIs(`/sessions/project/${projectId}/new`));
  },

  assertSessionSelected(sessionId: string) {
    cy.location("hash").should("match", hashIs(`/sessions/${sessionId}`));
  },

  // ---------------------------------------------------------------------------
  // What the daemon was asked
  // ---------------------------------------------------------------------------

  /** Asserts `backend` was sent exactly one `StartSession`, carrying at least `expected`. */
  assertStartedOnce(backend: RecordingBackend, expected: Record<string, unknown>) {
    cy.wrap(backend).should((b) => {
      const calls = b.callsTo(SessionService.method.startSession);
      expect(calls, "one StartSession request").to.have.length(1);
      expect(calls[0]).to.include(expected);
    });
  },
};
