/**
 * Acceptance: a drawer row whose session holds the `orchestrator` role shows an *orchestrator*
 * marker; an ordinary row does not (`#orchestrators` 1/11). The drawer is fed its sessions as
 * props — it makes no RPC of its own here.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-orchestrator-sessions.md
 * Changeset: docs/dev/1-WIP/2026-11-10-orchestrator-role.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { SessionDrawer } from "../../src/SessionDrawer";
import { TooltipProvider } from "tddy-ui-web";
import { byTestId, sessionsDrawerItem, sessionsDrawerItemOrchestrator } from "../support/testIds";

const PROJECT = { projectId: "proj-alpha", name: "alpha" };

function aSession(sessionId: string, overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    projectId: PROJECT.projectId,
    createdAt: "2026-11-10T10:00:00Z",
    status: "active",
    isActive: true,
    pid: 4242,
    ...overrides,
  });
}

const AN_ORCHESTRATOR = aSession("orch-1", {
  sessionRole: "orchestrator",
  managedCodebase: true,
  model: "opus",
});
const AN_ORDINARY_SESSION = aSession("ordinary-1");

function aDrawerListing(sessions: SessionEntry[]) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions}
        projects={[PROJECT]}
        selectedSessionId={null}
        onSelectSession={cy.stub()}
        isOpen
        onClose={cy.stub()}
        onOpen={cy.stub()}
        crossHostSessionsVisible
      />
    </TooltipProvider>,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
});

it("a session with the orchestrator role shows the orchestrator marker", () => {
  // When
  aDrawerListing([AN_ORCHESTRATOR, AN_ORDINARY_SESSION]);

  // Then the orchestrator's row carries the marker, inside the row itself
  byTestId(sessionsDrawerItem(AN_ORCHESTRATOR.sessionId))
    .find(`[data-testid='${sessionsDrawerItemOrchestrator(AN_ORCHESTRATOR.sessionId)}']`)
    .should("be.visible")
    .and("contain.text", "orchestrator");
  // And the ordinary row does not
  byTestId(sessionsDrawerItem(AN_ORDINARY_SESSION.sessionId)).should("be.visible");
  byTestId(sessionsDrawerItemOrchestrator(AN_ORDINARY_SESSION.sessionId)).should("not.exist");
});
