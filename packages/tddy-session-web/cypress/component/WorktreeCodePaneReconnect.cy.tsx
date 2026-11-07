/**
 * The Code pane keeps its `code_navigation.CodeSession` for as long as it is mounted: a session that
 * ends under it — a transport blip, a daemon restart — is reopened, after 1 s and then twice as long
 * after each failed attempt, up to 30 s, back to 1 s once a reopened session has delivered and stayed
 * open for 10 s — so a host that accepts a session and drops it at once is backed off too. The
 * strip says it is reconnecting meanwhile, and queries still waiting are asked again on the new
 * session. A session the host refuses is not reopened; unmounting stops reopening.
 *
 * Feature: docs/ft/web/session-code-pane.md (Web, Code pane);
 * reconnect decided 2026-10-18 (changeset Decisions & Trade-offs).
 *
 * Timing is pinned with `cy.clock`, so every delay is exact and nothing waits in real time.
 */

import "../../src/index.css";
import { anIndexLoading } from "../support/rpc/codeSessionBackend";
import { expectTheHost } from "../support/rpc/codeSessionAssertions";
import { aCodePaneOn, THE_WORKTREE } from "../support/drivers/worktreeCodePaneDriver";
import { worktreeCodePanePage } from "../support/pages/worktreeCodePanePage";
import {
  AREA_TYPE,
  INDEXING_38,
  THE_CALL_TO_AREA,
  aGeometryWorktreeHost,
  inMainRs,
} from "../support/fixtures/geometryWorktree";

const A_RESTART = "the daemon restarted";

/** The pane's session attempts the host has seen: one `open` each. */
const attempts = (count: number) => Array.from({ length: count }, () => THE_WORKTREE);

/** Why the `n`th reopening attempt (one-based) fails. */
const failedAttempt = (n: number) => `attempt ${n} found the daemon still down`;

/**
 * The strip says the session is being reopened, and why it last dropped (default: the restart).
 * Also the point at which the pane has heard of the drop and is waiting to reopen.
 */
function expectTheStripReconnecting(reason: string = A_RESTART) {
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "reconnecting");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Reconnecting…");
  worktreeCodePanePage.indexStatusDetail().should("have.text", reason);
}

/** The strip follows the index again: indexing at 38%. */
function expectTheStripIndexing() {
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "progress");
  worktreeCodePanePage.indexStatusSummary().should("have.text", "Indexing — Indexing 38%");
}

beforeEach(() => {
  cy.viewport(1280, 900);
});

it("a session dropped under the pane is reopened after a second, saying it is reconnecting meanwhile", () => {
  // Given an open pane following an index that is indexing
  cy.clock();
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  aCodePaneOn(host).mount();
  expectTheStripIndexing();

  // When the host drops the session
  cy.then(() => host.dropSessions(A_RESTART));

  // Then the strip says it is reconnecting, and nothing is reopened before a second has passed
  expectTheStripReconnecting();
  cy.tick(999);
  expectTheHost(host).toHaveOpened(attempts(1));

  // When the second has passed
  cy.tick(1);

  // Then the session is reopened on the same worktree, and the strip follows the index again
  expectTheHost(host).toHaveOpened(attempts(2));
  expectTheStripIndexing();
});

it("waits twice as long after each failed attempt, up to 30 seconds", () => {
  // Given an open pane, and a host that will fail the next six attempts at once, each its own way
  cy.clock();
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  aCodePaneOn(host).mount();
  expectTheStripIndexing();
  cy.then(() => host.failingNextOpens(...[1, 2, 3, 4, 5, 6].map(failedAttempt)));

  // When the host drops the session
  cy.then(() => host.dropSessions(A_RESTART));
  expectTheStripReconnecting();

  // Then the six failing attempts follow the drop, and each other, by 1, 2, 4, 8, 16 and 30 seconds
  const waitsBeforeFailingAttemptsMs = [1_000, 2_000, 4_000, 8_000, 16_000, 30_000];
  waitsBeforeFailingAttemptsMs.forEach((waitMs, i) => {
    cy.tick(waitMs - 1);
    expectTheHost(host).toHaveOpened(attempts(i + 1));
    cy.tick(1);
    expectTheHost(host).toHaveOpened(attempts(i + 2));
    // (the pane has heard this attempt fail before the next wait begins)
    expectTheStripReconnecting(failedAttempt(i + 1));
  });
  // And the seventh, which the host accepts, follows the sixth by 30 seconds too
  cy.tick(29_999);
  expectTheHost(host).toHaveOpened(attempts(7));
  cy.tick(1);
  expectTheHost(host).toHaveOpened(attempts(8));
  expectTheStripIndexing();
});

it("a reopened session that stayed open for 10 seconds waits a second again after the next drop", () => {
  // Given a pane whose session failed to reopen once before it was reopened
  cy.clock();
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  aCodePaneOn(host).mount();
  expectTheStripIndexing();
  cy.then(() => host.failingNextOpens(failedAttempt(1)));
  cy.then(() => host.dropSessions(A_RESTART));
  expectTheStripReconnecting();
  cy.tick(1_000);
  expectTheStripReconnecting(failedAttempt(1));
  cy.tick(2_000);
  expectTheHost(host).toHaveOpened(attempts(3));
  expectTheStripIndexing();

  // When the reopened session stays open for 10 seconds, then the host drops it
  cy.tick(10_000);
  cy.then(() => host.dropSessions(A_RESTART));
  expectTheStripReconnecting();

  // Then it is reopened a second later, not four
  cy.tick(999);
  expectTheHost(host).toStillHaveOpened(attempts(3));
  cy.tick(1);
  expectTheHost(host).toHaveOpened(attempts(4));
});

it("a host that accepts each session and drops it at once is backed off, not asked every second", () => {
  // Given an open pane following an index that is indexing
  cy.clock();
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  aCodePaneOn(host).mount();
  expectTheStripIndexing();

  // When the host drops every session right after it reported progress on it
  // Then each reopening follows the last drop by twice the wait before it: 1, 2, then 4 seconds
  const waitsBeforeReopeningMs = [1_000, 2_000, 4_000];
  waitsBeforeReopeningMs.forEach((waitMs, i) => {
    cy.then(() => host.dropSessions(A_RESTART));
    expectTheStripReconnecting();
    cy.tick(waitMs - 1);
    expectTheHost(host).toStillHaveOpened(attempts(i + 1));
    cy.tick(1);
    expectTheHost(host).toHaveOpened(attempts(i + 2));
    expectTheStripIndexing();
  });
});

it("a hover held when the session dropped is asked again on the reopened one and fills in", () => {
  // Given main.rs previewed while the index loads, with a hover on the call to `area` held
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount().previewing("src/main.rs");
  pane.dwellOn(THE_CALL_TO_AREA);
  worktreeCodePanePage.hoverLoading().should("be.visible");

  // When the host drops the session and the pane reopens it
  cy.then(() => host.dropSessions(A_RESTART));
  expectTheHost(host).toHaveOpened(attempts(2));

  // Then the held hover was asked again on the new session
  expectTheHost(host).toHaveBeenAskedOfKind("HOVER", [
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
    { kind: "HOVER", ...inMainRs(THE_CALL_TO_AREA) },
  ]);

  // When the index finishes loading
  cy.then(() => host.finishLoading());

  // Then the card fills in with the type
  worktreeCodePanePage.hoverBody().should("have.text", AREA_TYPE);
});

it("closing the pane while it is reconnecting stops reopening", () => {
  // Given a pane whose session dropped and is waiting to be reopened
  cy.clock();
  const host = aGeometryWorktreeHost().startingAt(anIndexLoading(INDEXING_38));
  const pane = aCodePaneOn(host).mount();
  expectTheStripIndexing();
  cy.then(() => host.dropSessions(A_RESTART));
  expectTheStripReconnecting();

  // When the pane closes and a minute passes
  pane.close();
  cy.tick(60_000);

  // Then a pane opened afterwards is the only one to open a session again
  aCodePaneOn(host).mount();
  expectTheStripIndexing();
  expectTheHost(host).toHaveOpened(attempts(2));
});

it("a session the host refuses is not reopened, and the strip names why", () => {
  // Given a host that refuses every session on this worktree
  cy.clock();
  const host = aGeometryWorktreeHost()
    .startingAt(anIndexLoading(INDEXING_38))
    .refusingEveryOpen("the worktree is not listed for this project");

  // When the pane opens and a minute passes
  aCodePaneOn(host).mount();
  worktreeCodePanePage.indexStatus().should("have.attr", "data-state", "failed");
  cy.tick(60_000);

  // Then it asked once, and the strip names the refusal
  expectTheHost(host).toHaveOpened(attempts(1));
  worktreeCodePanePage
    .indexStatusSummary()
    .should("have.text", "Indexing failed — the worktree is not listed for this project");
});
