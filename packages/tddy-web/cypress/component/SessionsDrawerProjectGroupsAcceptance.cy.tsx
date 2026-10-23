/**
 * Acceptance: the expanded sessions drawer groups sessions per project — a collapsible group per
 * project ordered by name, a trailing **Other** group, today's Active/Remaining split inside each
 * group, a collapsed set that survives a remount, and a selected session's group expanding.
 *
 * PRD: docs/ft/web/session-drawer.md (`#project-home` 2/7).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { SessionDrawer } from "../../src/components/sessions/SessionDrawer";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import {
  byTestId,
  sessionsDrawerItem,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectGroupHeader,
  TEST_IDS,
} from "../support/testIds";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };

function aConnectedSession(sessionId: string, projectId: string): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    projectId,
    createdAt: "2026-10-22T10:00:00Z",
    status: "active",
    repoPath: `/home/dev/${sessionId}`,
    pid: 4242,
    isActive: true,
  });
}

function aDisconnectedSession(sessionId: string, projectId: string): SessionEntry {
  return { ...aConnectedSession(sessionId, projectId), isActive: false, status: "exited", pid: 0 };
}

function aGroupedDrawer(sessions: SessionEntry[], selectedSessionId: string | null = null) {
  const drawer = {
    mount() {
      cy.mount(
        <TooltipProvider delayDuration={0}>
          <SessionDrawer
            sessions={sessions}
            projects={[BETA, ALPHA]}
            selectedSessionId={selectedSessionId}
            onSelectSession={cy.stub().as("onSelectSession")}
            isOpen
            onClose={cy.stub()}
            onOpen={cy.stub()}
            crossHostSessionsVisible
          />
        </TooltipProvider>,
      );
      return drawer;
    },
    group: (key: string) => byTestId(sessionsDrawerProjectGroup(key)),
    header: (key: string) => byTestId(sessionsDrawerProjectGroupHeader(key)),
    row: (sessionId: string) => byTestId(sessionsDrawerItem(sessionId)),
    groupOrder: (): Cypress.Chainable<string[]> =>
      cy
        .get("[data-testid^='sessions-drawer-project-group-header-']")
        .then(($h) => [...$h].map((el) => el.getAttribute("data-testid")!.replace("sessions-drawer-project-group-header-", ""))),
  };
  return drawer;
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
});

it("the drawer shows one collapsible group per project", () => {
  // Given
  const drawer = aGroupedDrawer([
    aConnectedSession("b1", BETA.projectId),
    aConnectedSession("a1", ALPHA.projectId),
    aConnectedSession("a2", ALPHA.projectId),
    aConnectedSession("orphan", ""),
  ]).mount();

  // Then — ordered by name, Other last, each header naming its project and count
  drawer.groupOrder().should("deep.equal", [ALPHA.projectId, BETA.projectId, "other"]);
  drawer.header(ALPHA.projectId).should("contain", "alpha").and("contain", "2");
  drawer.header("other").should("contain", "Other").and("contain", "1");
  drawer.group(ALPHA.projectId).within(() => {
    byTestId(sessionsDrawerItem("a1")).should("exist");
    byTestId(sessionsDrawerItem("a2")).should("exist");
  });
});

it("a collapsed project group stays collapsed after a remount", () => {
  // Given
  const sessions = [aConnectedSession("a1", ALPHA.projectId), aConnectedSession("b1", BETA.projectId)];
  const drawer = aGroupedDrawer(sessions).mount();

  // When
  drawer.header(ALPHA.projectId).click();
  drawer.row("a1").should("not.be.visible");
  aGroupedDrawer(sessions).mount();

  // Then
  drawer.row("a1").should("not.be.visible");
  drawer.row("b1").should("be.visible");
});

it("active and remaining separators appear inside each project group", () => {
  // Given
  const drawer = aGroupedDrawer([
    aConnectedSession("a-live", ALPHA.projectId),
    aDisconnectedSession("a-done", ALPHA.projectId),
    aConnectedSession("b-live", BETA.projectId),
  ]).mount();

  // Then — alpha has both partitions, beta only active sessions (no separators)
  drawer.group(ALPHA.projectId).within(() => {
    byTestId(TEST_IDS.sessionsDrawerSeparatorActive).should("exist");
    byTestId(TEST_IDS.sessionsDrawerSeparatorRemaining).should("exist");
  });
  drawer.group(BETA.projectId).within(() => {
    byTestId(TEST_IDS.sessionsDrawerSeparatorActive).should("not.exist");
  });
});

it("selecting a session in a collapsed group expands it", () => {
  // Given alpha collapsed
  const sessions = [aConnectedSession("a1", ALPHA.projectId), aConnectedSession("b1", BETA.projectId)];
  aGroupedDrawer(sessions).mount().header(ALPHA.projectId).click();

  // When a1 becomes the selected session
  const drawer = aGroupedDrawer(sessions, "a1").mount();

  // Then its group is open
  drawer.row("a1").should("be.visible");
});
