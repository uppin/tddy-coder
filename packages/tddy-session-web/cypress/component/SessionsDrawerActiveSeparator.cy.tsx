/**
 * Acceptance test: the open sessions drawer lists its active sessions (green/yellow dots) bare, with
 * no header, and folds the rest (grey/disconnected) under a collapsible **More**, collapsed by
 * default. Bulk-delete selection spans both.
 *
 * PRD: docs/ft/web/session-drawer.md § Active / Remaining Partition Separator
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { SessionAgentActivitySchema } from "tddy-client-web/gen/types_pb";
import { SessionDrawer } from "../../src/SessionDrawer";
import { TooltipProvider } from "tddy-ui-web";
import { byTestId, sessionsDrawerItem, sessionRowSelect, TEST_IDS } from "../support/testIds";

// ---------------------------------------------------------------------------
// Session builders — a green (connected) session by default; overrides pick the dot.
// ---------------------------------------------------------------------------

/** A connected (green-dot) session. */
function aConnectedSession(sessionId: string): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    createdAt: "2026-07-24T10:00:00Z",
    status: "active",
    repoPath: `/home/dev/${sessionId}`,
    pid: 12345,
    isActive: true,
    projectId: "proj-1",
    pendingElicitation: false,
  });
}

/** A needs-input (yellow-dot) session — alive but blocked waiting for the operator. */
function aNeedsInputSession(sessionId: string): SessionEntry {
  return { ...aConnectedSession(sessionId), pendingElicitation: true };
}

/** A disconnected (grey-dot) session — finished/history. */
function aDisconnectedSession(sessionId: string): SessionEntry {
  return { ...aConnectedSession(sessionId), isActive: false, status: "exited", pid: 0 };
}

/** A connected session started at `createdAt`, by `orchestratorSessionId` when given. */
function aSessionStartedAt(sessionId: string, createdAt: string, orchestratorSessionId = ""): SessionEntry {
  return { ...aConnectedSession(sessionId), createdAt, orchestratorSessionId };
}

/** A disconnected session last updated at `updatedAt`, its agent last seen at `lastActivityAt`. */
function aSessionEndedAt(sessionId: string, updatedAt: string, lastActivityAt?: string): SessionEntry {
  return {
    ...aDisconnectedSession(sessionId),
    updatedAt,
    lastActivity:
      lastActivityAt === undefined
        ? undefined
        : create(SessionAgentActivitySchema, { atUnixMs: BigInt(Date.parse(lastActivityAt)), summary: "edited a file" }),
  };
}

/** The ids among `sessionIds` in the order their drawer rows appear in the DOM, shown or folded. */
function rowOrderOf(sessionIds: string[]): Cypress.Chainable<string[]> {
  return cy.get("[data-testid^='sessions-drawer-item-']").then(($rows) =>
    [...$rows]
      .map((el) => el.getAttribute("data-testid")!.replace(/^sessions-drawer-item-/, ""))
      .filter((id) => sessionIds.includes(id)),
  );
}

// ---------------------------------------------------------------------------
// Fluent driver — encapsulates mounting + the drawer's separator selectors.
// ---------------------------------------------------------------------------

interface DrawerOptions {
  selectionMode?: boolean;
  selectedForDelete?: ReadonlySet<string>;
  onToggleSelect?: (sessionId: string) => void;
}

function aSessionsDrawer(sessions: SessionEntry[], options: DrawerOptions = {}) {
  const driver = {
    mount() {
      cy.mount(
        <TooltipProvider delayDuration={0}>
          <SessionDrawer
            sessions={sessions}
            selectedSessionId={null}
            onSelectSession={cy.stub().as("onSelectSession")}
            isOpen
            onClose={cy.stub().as("onClose")}
            onOpen={cy.stub().as("onOpen")}
            crossHostSessionsVisible
            selectionMode={options.selectionMode ?? false}
            selectedForDelete={options.selectedForDelete}
            onToggleSelect={options.onToggleSelect}
          />
        </TooltipProvider>,
      );
      return driver;
    },
    activeSeparator: () => byTestId(TEST_IDS.sessionsDrawerSeparatorActive),
    remainingSeparator: () => byTestId(TEST_IDS.sessionsDrawerSeparatorRemaining),
    row: (sessionId: string) => byTestId(sessionsDrawerItem(sessionId)),
    rowCheckbox: (sessionId: string) => byTestId(sessionRowSelect(sessionId)),
    clickActiveSeparator() {
      driver.activeSeparator().click();
      return driver;
    },
    clickRemainingSeparator() {
      driver.remainingSeparator().click();
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("Sessions drawer — active / remaining separator", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("lists active sessions with no header and the rest under More", () => {
    // Given — two live sessions (green + yellow) and one disconnected (grey)
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aNeedsInputSession("live-yellow"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then
    drawer.activeSeparator().should("not.exist");
    drawer.row("live-green").should("be.visible");
    drawer.row("live-yellow").should("be.visible");
    drawer.remainingSeparator().invoke("text").should("equal", "More (1)");
  });

  it("collapses the remaining partition by default", () => {
    // Given — one live and one disconnected session
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then — the disconnected row is hidden under the collapsed Remaining header
    drawer.row("done-grey").should("not.be.visible");
  });

  it("expands the remaining partition when its separator is clicked", () => {
    // Given — a collapsed Remaining partition
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aDisconnectedSession("done-grey"),
    ]).mount();
    drawer.row("done-grey").should("not.be.visible");

    // When — the operator clicks the Remaining header
    drawer.clickRemainingSeparator();

    // Then — the disconnected row becomes visible
    drawer.row("done-grey").should("be.visible");
  });

  it("places a needs-input (yellow) session in the expanded active partition", () => {
    // Given — a needs-input session alongside a disconnected one
    const drawer = aSessionsDrawer([
      aNeedsInputSession("needs-me"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then — the yellow session counts as active and is visible by default
    drawer.row("needs-me").should("be.visible");
    drawer.row("done-grey").should("not.be.visible");
  });

  it("renders a plain list with no separators when every session is active", () => {
    // Given — two live sessions and nothing disconnected
    const drawer = aSessionsDrawer([
      aConnectedSession("live-1"),
      aConnectedSession("live-2"),
    ]).mount();

    // Then — no partition headers render; both rows show in a flat list
    byTestId(TEST_IDS.sessionsDrawerSeparatorActive).should("not.exist");
    byTestId(TEST_IDS.sessionsDrawerSeparatorRemaining).should("not.exist");
    drawer.row("live-1").should("be.visible");
    drawer.row("live-2").should("be.visible");
  });

  it("folds every session under a collapsed More when none is active", () => {
    // Given — a project whose sessions have all ended
    const drawer = aSessionsDrawer([
      aDisconnectedSession("done-1"),
      aDisconnectedSession("done-2"),
    ]).mount();

    // Then — the ended rows sit under More, collapsed like any other
    drawer.remainingSeparator().invoke("text").should("equal", "More (2)");
    drawer.row("done-1").should("not.be.visible");
    drawer.row("done-2").should("not.be.visible");

    // When — the operator opens More
    drawer.clickRemainingSeparator();

    // Then
    drawer.row("done-1").should("be.visible");
    drawer.row("done-2").should("be.visible");
  });

  it("lists active sessions newest-started first, each started session right after its starter", () => {
    // Given — live sessions, one orchestrator having started two of them
    const sessions = [
      aSessionStartedAt("oldest", "2026-11-20T08:00:00Z"),
      aSessionStartedAt("orch", "2026-11-20T09:00:00Z"),
      aSessionStartedAt("orch-kid-1", "2026-11-20T10:00:00Z", "orch"),
      aSessionStartedAt("orch-kid-2", "2026-11-20T11:00:00Z", "orch"),
      aSessionStartedAt("newest", "2026-11-20T12:00:00Z"),
    ];

    // When
    aSessionsDrawer(sessions).mount();

    // Then
    rowOrderOf(sessions.map((s) => s.sessionId)).should("deep.equal", [
      "newest",
      "orch",
      "orch-kid-2",
      "orch-kid-1",
      "oldest",
    ]);
  });

  it("lists the sessions under More most recently stopped or active first", () => {
    // Given — ended sessions; the latest of each one's update and agent activity places it
    const sessions = [
      aSessionEndedAt("ended-long-ago", "2026-11-01T00:00:00Z", "2026-11-02T00:00:00Z"),
      aSessionEndedAt("stopped-just-now", "2026-11-24T12:00:00Z", "2026-11-20T00:00:00Z"),
      aSessionEndedAt("busy-till-late", "2026-11-21T00:00:00Z", "2026-11-24T11:00:00Z"),
      aSessionEndedAt("never-active", "2026-11-23T00:00:00Z"),
    ];

    // When
    aSessionsDrawer([aConnectedSession("live"), ...sessions]).mount();

    // Then
    rowOrderOf(sessions.map((s) => s.sessionId)).should("deep.equal", [
      "stopped-just-now",
      "busy-till-late",
      "never-active",
      "ended-long-ago",
    ]);
  });

  it("keeps remaining-partition checkboxes reachable during bulk selection", () => {
    // Given — selection mode on, with a disconnected row in the (otherwise collapsed) Remaining partition
    const drawer = aSessionsDrawer(
      [aConnectedSession("live-green"), aDisconnectedSession("done-grey")],
      {
        selectionMode: true,
        selectedForDelete: new Set<string>(),
        onToggleSelect: cy.stub().as("onToggleSelect"),
      },
    ).mount();

    // The list is partitioned (a Remaining header exists) yet selection mode keeps it expanded
    drawer.remainingSeparator().should("exist");

    // When — the operator ticks the disconnected row's checkbox
    drawer.rowCheckbox("done-grey").should("be.visible").click();

    // Then — selection spans the Remaining partition (the toggle fires with its id)
    cy.get("@onToggleSelect").should("have.been.calledWith", "done-grey");
  });
});
