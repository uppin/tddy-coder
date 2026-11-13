/**
 * Acceptance: a drawer row whose session an orchestrator marked shows the mark as a badge, with the
 * orchestrator's note on hover; an unmarked row shows none (`#orchestrators` 5/11). The drawer is
 * fed its sessions as props — it makes no RPC of its own here.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-lifecycle-tools.md
 * Changeset: docs/dev/1-WIP/2026-11-10-session-lifecycle-tools.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import {
  OrchestrationMarkSchema,
  SessionEntrySchema,
  type SessionEntry,
} from "tddy-client-web/gen/session_pb";
import { SessionDrawer } from "../../src/SessionDrawer";
import { TooltipProvider } from "tddy-ui-web";
import { byTestId, sessionsDrawerItem, sessionsDrawerItemMark } from "../support/testIds";

const PROJECT = { projectId: "proj-alpha", name: "alpha" };
const BLOCKED_NOTE = "waiting on #881 to land";

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

function markedAs(mark: string, note: string) {
  return create(OrchestrationMarkSchema, {
    mark,
    note,
    setBySessionId: "orch-1",
    atUnixMs: BigInt(1_794_000_000_000),
  });
}

const A_BLOCKED_SESSION = aSession("blocked-1", {
  orchestrationMark: markedAs("blocked", BLOCKED_NOTE),
});
const A_PAUSED_SESSION = aSession("paused-1", {
  isActive: false,
  pid: 0,
  orchestrationMark: markedAs("paused", "out of budget for today"),
});
const AN_UNMARKED_SESSION = aSession("unmarked-1");

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

it("a marked session shows its mark with the note on hover", () => {
  // When
  aDrawerListing([A_BLOCKED_SESSION, A_PAUSED_SESSION, AN_UNMARKED_SESSION]);

  // Then the blocked row carries its mark inside the row, the note as its hover text
  byTestId(sessionsDrawerItem(A_BLOCKED_SESSION.sessionId))
    .find(`[data-testid='${sessionsDrawerItemMark(A_BLOCKED_SESSION.sessionId)}']`)
    .should("be.visible")
    .and("contain.text", "blocked")
    .and("have.attr", "title", BLOCKED_NOTE);
  // And the paused row carries its own mark
  byTestId(sessionsDrawerItemMark(A_PAUSED_SESSION.sessionId)).should("contain.text", "paused");
  // And the unmarked row has none
  byTestId(sessionsDrawerItem(AN_UNMARKED_SESSION.sessionId)).should("be.visible");
  byTestId(sessionsDrawerItemMark(AN_UNMARKED_SESSION.sessionId)).should("not.exist");
});
