/**
 * Component tests for SessionDrawer — stack-grouping behaviour.
 *
 * Tests verify that when sessions include PR-stack children (identified via
 * `orchestratorSessionId`, proto field 21), the drawer renders them collapsed under a
 * `<details>/<summary>` group rooted at the orchestrator session.
 */
import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "../../src/gen/connection_pb";
import { SessionDrawer } from "../../src/components/sessions/SessionDrawer";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { byTestId, sessionsDrawerItem, sessionsDrawerStackGroup } from "../support/testIds";

// ---------------------------------------------------------------------------
// Session factory helpers
// ---------------------------------------------------------------------------

/** Builds a plain (non-stack) session entry. */
function aSession(sessionId: string, extra: Record<string, unknown> = {}) {
  return {
    ...create(SessionEntrySchema, {
      sessionId,
      createdAt: "2026-06-26T10:00:00Z",
      status: "active",
      repoPath: "/home/dev/project",
      pid: 12345,
      isActive: true,
      projectId: "proj-1",
    }),
    ...extra,
  };
}

/** Builds a child session whose `orchestratorSessionId` references a parent. */
function aChildSession(sessionId: string, orchestratorSessionId: string) {
  return aSession(sessionId, { orchestratorSessionId });
}

// ---------------------------------------------------------------------------
// Mount helper
// ---------------------------------------------------------------------------

function mountDrawer(
  sessions: ReturnType<typeof aSession>[],
  {
    selectedSessionId = null,
    onSelectSession = cy.stub().as("onSelectSession"),
  }: {
    selectedSessionId?: string | null;
    onSelectSession?: (id: string) => void;
  } = {},
) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions as Parameters<typeof SessionDrawer>[0]["sessions"]}
        selectedSessionId={selectedSessionId}
        onSelectSession={onSelectSession}
      />
    </TooltipProvider>,
  );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("SessionDrawer — flat sessions (no stack)", () => {
  it("renders each non-stack session as a drawer item without a group wrapper", () => {
    // Given — two plain sessions
    const sessions = [
      aSession("sess-alpha"),
      aSession("sess-beta"),
    ];

    // When
    mountDrawer(sessions);

    // Then — both items render; no stack group wrapper exists
    cy.get(`[data-testid='${sessionsDrawerItem("sess-alpha")}']`).should("be.visible");
    cy.get(`[data-testid='${sessionsDrawerItem("sess-beta")}']`).should("be.visible");
    byTestId("sessions-drawer").within(() => {
      cy.get("details").should("not.exist");
    });
  });
});

describe("SessionDrawer — stack group rendering", () => {
  it("renders orchestrator + children in a collapsible <details> group", () => {
    // Given — one orchestrator and one child that references it
    const orchestrator = aSession("orch-session-1");
    const child = aChildSession("child-session-1", "orch-session-1");

    // When
    mountDrawer([orchestrator, child]);

    // Then — a collapsible <details> group exists for the orchestrator
    cy.get(`[data-testid='${sessionsDrawerStackGroup("orch-session-1")}']`).should("exist");

    // The group contains both the orchestrator item (in <summary>) and the child item
    cy.get(`[data-testid='${sessionsDrawerStackGroup("orch-session-1")}']`).within(() => {
      cy.get(`[data-testid='${sessionsDrawerItem("orch-session-1")}']`).should("exist");
      cy.get(`[data-testid='${sessionsDrawerItem("child-session-1")}']`).should("exist");
    });
  });

  it("child items are rendered with a depth indent (depth={1})", () => {
    // Given
    const orch = aSession("orch-indent");
    const child = aChildSession("child-indent", "orch-indent");

    // When
    mountDrawer([orch, child]);

    // Then — child item element has a data-depth attribute (or padding-left) indicating depth 1
    // (Exact implementation: data-testid="sessions-drawer-item-{id}" with data-depth="1"
    //  OR a CSS class with left padding — we assert data-depth here as the contract)
    cy.get(`[data-testid='${sessionsDrawerItem("child-indent")}']`)
      .should("have.attr", "data-depth", "1");
  });

  it("collapsing the <details> group hides child items", () => {
    // Given — a group that is initially open (default: open)
    const orch = aSession("orch-collapsible");
    const child = aChildSession("child-collapsible", "orch-collapsible");

    // When — mounted (open by default)
    mountDrawer([orch, child]);

    // Child should be visible initially
    cy.get(`[data-testid='${sessionsDrawerItem("child-collapsible")}']`).should("be.visible");

    // When — user clicks the <summary> to collapse the group
    cy.get(`[data-testid='${sessionsDrawerStackGroup("orch-collapsible")}'] summary`).click();

    // Then — child item is no longer visible
    cy.get(`[data-testid='${sessionsDrawerItem("child-collapsible")}']`).should("not.be.visible");
  });

  it("selecting a child session fires onSelectSession with the child session id", () => {
    // Given
    const orch = aSession("orch-select");
    const child = aChildSession("child-select", "orch-select");

    // When
    mountDrawer([orch, child]);

    // Clicking the child item
    cy.get(`[data-testid='${sessionsDrawerItem("child-select")}']`).click();

    // Then — onSelectSession called with child id (not orchestrator id)
    cy.get("@onSelectSession").should("have.been.calledWith", "child-select");
  });

  it("orphan child (missing orchestrator) is rendered in flat without a group", () => {
    // Given — a child whose orchestratorSessionId points to a non-present session
    const orphan = aChildSession("orphan-sess", "missing-orch-999");

    // When
    mountDrawer([orphan]);

    // Then — rendered as a flat item, no group
    cy.get(`[data-testid='${sessionsDrawerItem("orphan-sess")}']`).should("be.visible");
    byTestId("sessions-drawer").within(() => {
      cy.get("details").should("not.exist");
    });
  });

  it("multiple independent stack groups are each wrapped in their own <details>", () => {
    // Given — two independent orchestrators with one child each
    const orch1 = aSession("orch-multi-1");
    const child1 = aChildSession("child-of-multi-1", "orch-multi-1");
    const orch2 = aSession("orch-multi-2");
    const child2 = aChildSession("child-of-multi-2", "orch-multi-2");

    // When
    mountDrawer([orch1, child1, orch2, child2]);

    // Then — two separate groups exist
    cy.get(`[data-testid='${sessionsDrawerStackGroup("orch-multi-1")}']`).should("exist");
    cy.get(`[data-testid='${sessionsDrawerStackGroup("orch-multi-2")}']`).should("exist");
    byTestId("sessions-drawer").within(() => {
      cy.get("details").should("have.length", 2);
    });
  });
});
