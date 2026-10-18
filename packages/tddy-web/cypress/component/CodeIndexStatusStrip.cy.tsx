/**
 * The Code pane's bottom strip reads where its worktree's code index stands, as the pane's
 * `code_navigation.CodeSession` reports it. The loading → ready progression is
 * `WorktreeCodePaneCodeSession.cy.tsx`'s; this spec pins the strip's other readings: a worktree with
 * no index, a failed load, an index let go while the pane is open, and a session being reopened.
 *
 * Feature: docs/ft/web/session-code-pane.md (Indexing indicator);
 * docs/ft/web/session-code-pane.md.
 */

import {
  anIndexIdle,
  anIndexLoading,
  noIndexForTheWorktree,
} from "../support/rpc/codeSessionBackend";
import { aCodePaneOn } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import { INDEXING_38, aGeometryWorktreeHost } from "../support/fixtures/geometryWorktree";

/** A pane open on a worktree whose index is indexing, the strip already saying so. */
function aPaneFollowingAnIndexingIndex() {
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  aCodePaneOn(host).mount();
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
  return host;
}

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("says a worktree that is not a Rust worktree has no code index, and why", () => {
  // Given a worktree the host has no code index for
  const host = aGeometryWorktreeHost().startingAt(
    noIndexForTheWorktree("this worktree has no Cargo.toml at its root"),
  );

  // When the pane opens on it
  aCodePaneOn(host).mount();

  // Then the strip says there is no index, with the host's reason — never that one is starting
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "none");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "No code index for this worktree");
  worktreeCodePanePage
    .indexStatusDetail()
    .should("have.text", "this worktree has no Cargo.toml at its root");
});

it("names the reason when the index fails to load", () => {
  // Given a pane following an index that is indexing
  const host = aPaneFollowingAnIndexingIndex();

  // When the index fails to load
  cy.then(() => host.failLoading("rust-analyzer exited: no toolchain"));

  // Then the strip names the failure and its reason
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "failed");
  worktreeCodePanePage
    .indexStatusSummary()
    .should("have.text", "Indexing failed — rust-analyzer exited: no toolchain");
  worktreeCodePanePage.indexStatusDetail().should("not.exist");
});

it("an index let go while the pane is open reads as starting, never as not loaded", () => {
  // Given a pane following an index that is indexing
  const host = aPaneFollowingAnIndexingIndex();

  // When the host reports the index idle — its server let go, about to be warmed again
  cy.then(() => host.reportProgress(anIndexIdle()));

  // Then the strip says the index is starting
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "idle");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Code index starting…");

  // When the reload begins
  cy.then(() => host.reportProgress(anIndexLoading(INDEXING_38)));

  // Then the strip shows its progress
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
});

it("says it is reconnecting, and why, while the code session is down", () => {
  // Given a pane following an index that is indexing
  const host = aPaneFollowingAnIndexingIndex();

  // When the host drops the code session
  cy.then(() => host.dropSessions("index daemon unreachable"));

  // Then the strip says it is reconnecting and names why the session dropped
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "reconnecting");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Reconnecting…");
  worktreeCodePanePage.indexStatusDetail().should("have.text", "index daemon unreachable");
});
