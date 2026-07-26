/**
 * Acceptance tests: "Planned PRs" is a dismissible panel to the right of the chat.
 *
 * The list used to be a hardcoded `w-1/2` column, permanently halving the chat on desktop and
 * unusable beside it on mobile, with no way to dismiss it. It becomes a panel with the same contract
 * as the Session Inspector drawer: always mounted, `data-state` driving visibility, docked as a
 * narrow column on desktop and a full-screen overlay on mobile, toggleable on both.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md (C4, D10–D11).
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-9999-0000-0000-0000-000000000090";

const ORCHESTRATOR_SESSION: Partial<SessionEntry> = {
  sessionId: ORCHESTRATOR_SESSION_ID,
  createdAt: "2026-07-26T09:50:00Z",
  status: "idle",
  repoPath: "/home/dev/pr-stack-project",
  isActive: false,
  projectId: "proj-pr-stack",
  recipe: "pr-stack",
  stackPlanJson: aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Start-session attachment proto" }),
    aPlannedNode({ nodeId: "n2", title: "Session attachment storage" }),
  ]),
};

function openPrStackScreen() {
  mountWithRpc(
    withSelectedDaemon(<SessionsDrawerScreen />),
    aSessionsDrawerBackend([ORCHESTRATOR_SESSION]),
  );
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
}

/**
 * Mobile entry point. The session list defaults to closed below the `md` breakpoint
 * (`SessionsDrawerScreen`'s `sessionListOpen` is seeded from `detectIsMobile()`), so the list has to
 * be revealed before a session can be picked; selecting one closes it again.
 */
function openPrStackScreenOnMobile() {
  mountWithRpc(
    withSelectedDaemon(<SessionsDrawerScreen />),
    aSessionsDrawerBackend([ORCHESTRATOR_SESSION]),
  );
  sessionsDrawerPage.drawerOpenOverlayBtn().click();
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Desktop — docked open beside the chat
// ---------------------------------------------------------------------------

it("docks the Planned PRs panel open beside the chat on desktop", () => {
  // Given
  cy.viewport(1280, 800);

  // When
  openPrStackScreen();

  // Then — open by default, and narrower than the screen so the chat keeps its own space
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "open");
  prStackScreenPage.chat().should("exist");
  prStackScreenPage.expectPanelNarrowerThanScreen();
});

it("renders the planned-PR rows inside the panel", () => {
  // Given
  cy.viewport(1280, 800);

  // When
  openPrStackScreen();

  // Then — the list moved into the panel rather than staying a sibling column of the chat
  prStackScreenPage.panelPlannedPrList().should("exist");
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
});

it("closes the Planned PRs panel when the toggle is used on desktop", () => {
  // Given
  cy.viewport(1280, 800);
  openPrStackScreen();
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "open");

  // When
  prStackScreenPage.togglePlannedPrPanel();

  // Then
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "closed");
  prStackScreenPage.plannedPrList().should("not.be.visible");
});

it("reopens the Planned PRs panel when the toggle is used again", () => {
  // Given — the panel has been closed
  cy.viewport(1280, 800);
  openPrStackScreen();
  prStackScreenPage.togglePlannedPrPanel();
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "closed");

  // When
  prStackScreenPage.togglePlannedPrPanel();

  // Then
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "open");
});

// ---------------------------------------------------------------------------
// Mobile — a full-screen overlay, closed by default
// ---------------------------------------------------------------------------

it("keeps the Planned PRs panel closed by default on mobile", () => {
  // Given
  cy.viewport(390, 844);

  // When
  openPrStackScreenOnMobile();

  // Then — the chat owns the whole screen until the panel is asked for
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "closed");
  prStackScreenPage.chat().should("be.visible");
});

it("opens the Planned PRs panel as a full-screen overlay on mobile", () => {
  // Given
  cy.viewport(390, 844);
  openPrStackScreenOnMobile();

  // When
  prStackScreenPage.togglePlannedPrPanel();

  // Then
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "open");
  prStackScreenPage.expectPanelSpansScreenWidth();
});

it("dismisses the mobile overlay from the panel's own close control", () => {
  // Given — the overlay is covering the chat
  cy.viewport(390, 844);
  openPrStackScreenOnMobile();
  prStackScreenPage.togglePlannedPrPanel();
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "open");

  // When
  prStackScreenPage.plannedPrPanelClose().click();

  // Then
  prStackScreenPage.plannedPrPanel().should("have.attr", "data-state", "closed");
});
