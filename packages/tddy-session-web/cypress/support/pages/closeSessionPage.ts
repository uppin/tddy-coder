/**
 * Page object for closing a session from its drawer row: the row's × / trash bin / spinner, the
 * close dialog, and the drawer's failure notice (docs/ft/web/session-drawer.md § Closing a session).
 *
 * All raw selectors live here; test bodies call named methods.
 */

import {
  byTestId,
  closeSessionDialogWorktree,
  sessionsDrawerCloseFailure,
  sessionsDrawerItem,
  sessionsDrawerItemClose,
  sessionsDrawerItemClosing,
  sessionsDrawerItemRemoveWorktree,
  sessionsDrawerProjectHome,
  TEST_IDS,
} from "../testIds";
import type { CloseSessionControls, RecordedCloseRequest } from "../rpc/closeSessionBackend";

const WAIT = { timeout: 5000 };

/** Wait until the drawer lists `sessionId`, so an absence asserted next is not a list still loading. */
function rowIsListed(sessionId: string) {
  byTestId(sessionsDrawerItem(sessionId), WAIT).should("exist");
}

export const closeSessionPage = {
  // ---------------------------------------------------------------------------
  // Row controls
  // ---------------------------------------------------------------------------

  /**
   * Open the **More** fold an ended row sits under, unless it is already open, so the row and its
   * controls are interactable.
   */
  revealEndedRow(sessionId: string) {
    const more = `[data-testid='${TEST_IDS.sessionsDrawerSeparatorRemaining}']`;
    byTestId(sessionsDrawerItem(sessionId), WAIT)
      .closest(`div:has(> ${more})`)
      .children(more)
      .then(($more) => {
        if ($more.attr("aria-expanded") !== "true") cy.wrap($more).click();
      });
  },

  /** Click the active row's ×. */
  clickClose(sessionId: string) {
    byTestId(sessionsDrawerItemClose(sessionId), WAIT).click();
  },

  /** Click the ended row's trash bin (opening its **More** fold first). */
  clickRemoveWorktree(sessionId: string) {
    closeSessionPage.revealEndedRow(sessionId);
    byTestId(sessionsDrawerItemRemoveWorktree(sessionId), WAIT).click();
  },

  expectCloseControl(sessionId: string) {
    byTestId(sessionsDrawerItemClose(sessionId), WAIT)
      .should("exist")
      .and("have.attr", "aria-label")
      .and("match", /^Close session /);
  },

  expectNoCloseControl(sessionId: string) {
    rowIsListed(sessionId);
    byTestId(sessionsDrawerItemClose(sessionId)).should("not.exist");
  },

  expectRemoveWorktreeControl(sessionId: string) {
    byTestId(sessionsDrawerItemRemoveWorktree(sessionId), WAIT)
      .should("exist")
      .and("have.attr", "aria-label")
      .and("match", /^Remove worktree of /);
  },

  expectNoRemoveWorktreeControl(sessionId: string) {
    rowIsListed(sessionId);
    byTestId(sessionsDrawerItemRemoveWorktree(sessionId)).should("not.exist");
  },

  /** The project's pinned main entry is shown, with neither a × nor a trash bin for its session. */
  expectPinnedMainHasNoCloseControls(projectId: string, homeSessionId: string) {
    byTestId(sessionsDrawerProjectHome(projectId), WAIT).should("exist");
    byTestId(sessionsDrawerItemClose(homeSessionId)).should("not.exist");
    byTestId(sessionsDrawerItemRemoveWorktree(homeSessionId)).should("not.exist");
  },

  /** The row shows its mini spinner, and no × in its place. */
  expectClosing(sessionId: string) {
    byTestId(sessionsDrawerItemClosing(sessionId), WAIT).should("exist");
    byTestId(sessionsDrawerItemClose(sessionId)).should("not.exist");
  },

  expectNotClosing(sessionId: string) {
    rowIsListed(sessionId);
    byTestId(sessionsDrawerItemClosing(sessionId), WAIT).should("not.exist");
  },

  // ---------------------------------------------------------------------------
  // Close dialog
  // ---------------------------------------------------------------------------

  expectDialogOpen() {
    byTestId(TEST_IDS.closeSessionDialog, WAIT).should("be.visible");
  },

  expectNoDialog() {
    byTestId(TEST_IDS.closeSessionDialog).should("not.exist");
  },

  expectWorktreeSectionCount(count: number) {
    byTestId(TEST_IDS.closeSessionDialog, WAIT)
      .find("[data-testid^='close-session-dialog-worktree-']")
      .should("have.length", count);
  },

  /** The `index`-th section shows every one of `texts`. */
  expectWorktreeSectionShows(index: number, ...texts: string[]) {
    for (const text of texts) {
      byTestId(closeSessionDialogWorktree(index), WAIT).should("contain.text", text);
    }
  },

  /** Somewhere in the dialog, every one of `texts` is shown. */
  expectDialogShows(...texts: string[]) {
    for (const text of texts) {
      byTestId(TEST_IDS.closeSessionDialog, WAIT).should("contain.text", text);
    }
  },

  confirm() {
    byTestId(TEST_IDS.closeSessionDialogConfirm, WAIT).click();
  },

  cancel() {
    byTestId(TEST_IDS.closeSessionDialogCancel, WAIT).click();
  },

  retry() {
    byTestId(TEST_IDS.closeSessionDialogRetry, WAIT).click();
  },

  expectConfirmDisabled() {
    byTestId(TEST_IDS.closeSessionDialogConfirm, WAIT).should("be.disabled");
  },

  expectConfirmEnabled() {
    byTestId(TEST_IDS.closeSessionDialogConfirm, WAIT).should("be.enabled");
  },

  expectChildrenChoiceOffered() {
    byTestId(TEST_IDS.closeSessionDialogChildrenLeave, WAIT).should("be.visible");
    byTestId(TEST_IDS.closeSessionDialogChildrenClose, WAIT).should("be.visible");
  },

  chooseCloseChildrenToo() {
    byTestId(TEST_IDS.closeSessionDialogChildrenClose, WAIT).click();
  },

  // ---------------------------------------------------------------------------
  // Drawer notice
  // ---------------------------------------------------------------------------

  /** The drawer's notice for the session's failed close shows every one of `texts`. */
  expectFailureNoticeShows(sessionId: string, ...texts: string[]) {
    for (const text of texts) {
      byTestId(sessionsDrawerCloseFailure(sessionId), WAIT).should("contain.text", text);
    }
  },

  // ---------------------------------------------------------------------------
  // What reached the daemon
  // ---------------------------------------------------------------------------

  /** Let a held close of `sessionId` finish, in command-queue order. */
  releaseClose(backend: CloseSessionControls, sessionId: string) {
    cy.then(() => backend.releaseClose(sessionId));
  },

  /** The close requests the daemon received equal `expected`, in order. */
  expectCloseRequests(backend: CloseSessionControls, expected: RecordedCloseRequest[]) {
    cy.wrap(null).should(() => {
      expect(backend.closeRequests()).to.deep.equal(expected);
    });
  },

  /** The daemon received close requests for exactly `sessionIds` (in order), with `confirmed` as
   *  given — whatever fingerprint they carried. */
  expectClosesRequested(
    backend: CloseSessionControls,
    expected: { sessionId: string; confirmed: boolean }[],
  ) {
    cy.wrap(null).should(() => {
      expect(
        backend.closeRequests().map(({ sessionId, confirmed }) => ({ sessionId, confirmed })),
      ).to.deep.equal(expected);
    });
  },

  /** The first close request is `first`; the rest are exactly `then`, in any order. */
  expectCloseRequestsParentThen(
    backend: CloseSessionControls,
    first: RecordedCloseRequest,
    then: RecordedCloseRequest[],
  ) {
    cy.wrap(null).should(() => {
      const requests = backend.closeRequests();
      expect(requests).to.have.length(1 + then.length);
      expect(requests[0]).to.deep.equal(first);
      expect(requests.slice(1)).to.have.deep.members(then);
    });
  },

  expectNoCloseRequest(backend: CloseSessionControls) {
    cy.wrap(null).should(() => {
      expect(backend.closeRequests()).to.deep.equal([]);
    });
  },

  /** `PreviewCloseSession` was asked exactly for `sessionIds`, in order. */
  expectPreviewsAskedFor(backend: CloseSessionControls, sessionIds: string[]) {
    cy.wrap(null).should(() => {
      expect(backend.previewCloseCalls()).to.deep.equal(sessionIds);
    });
  },
};
