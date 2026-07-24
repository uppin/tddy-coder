/**
 * Acceptance test: the open sessions drawer splits its list into an Active partition
 * (green/yellow dots) and a Remaining partition (grey/disconnected dots), separated by a
 * collapsible header labelled "Active (N)" / "Remaining (M)". Active is expanded by default,
 * Remaining collapsed. Bulk-delete selection spans both partitions.
 *
 * PRD: docs/ft/web/session-drawer.md § Active / Remaining Partition Separator
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/connection_pb";
import { SessionDrawer } from "../../src/components/sessions/SessionDrawer";
import { TooltipProvider } from "../../src/components/ui/tooltip";
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

  it("labels the partitions Active (N) and Remaining (M) with live counts", () => {
    // Given — two live sessions (green + yellow) and one disconnected (grey)
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aNeedsInputSession("live-yellow"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then — the active header counts both live dots, the remaining header counts the grey one
    drawer.activeSeparator().should("contain.text", "Active (2)");
    drawer.remainingSeparator().should("contain.text", "Remaining (1)");
  });

  it("shows the active partition expanded by default", () => {
    // Given — one live and one disconnected session
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then — the Active header renders and its row is visible without any interaction
    drawer.activeSeparator().should("exist");
    drawer.row("live-green").should("be.visible");
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

  it("collapses the active partition when its separator is clicked", () => {
    // Given — an expanded Active partition
    const drawer = aSessionsDrawer([
      aConnectedSession("live-green"),
      aDisconnectedSession("done-grey"),
    ]).mount();
    drawer.row("live-green").should("be.visible");

    // When — the operator clicks the Active header
    drawer.clickActiveSeparator();

    // Then — the active row is hidden
    drawer.row("live-green").should("not.be.visible");
  });

  it("places a needs-input (yellow) session in the expanded active partition", () => {
    // Given — a needs-input session alongside a disconnected one
    const drawer = aSessionsDrawer([
      aNeedsInputSession("needs-me"),
      aDisconnectedSession("done-grey"),
    ]).mount();

    // Then — the yellow session counts as active and is visible by default
    drawer.activeSeparator().should("contain.text", "Active (1)");
    drawer.remainingSeparator().should("contain.text", "Remaining (1)");
    drawer.row("needs-me").should("be.visible");
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
