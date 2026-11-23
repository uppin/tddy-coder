/**
 * Acceptance: the work queue is a right sidebar of the session view, beside the Agent Activity
 * sidebar, behind its own toggle. The two are mutually exclusive, and a session that owns a queue
 * opens on it.
 *
 * Changeset: docs/dev/1-WIP/2026-11-23-workqueue-sidebar-ux.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "tddy-client-web/gen/activity_pb";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { SessionWithActivitySidebar } from "tddy-session-web";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { byTestId } from "../support/testIds";
import { aReplayBackend, replayAgentText } from "../support/rpc/acpReplay";

function aSession(ownsWorkQueue: boolean): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: ownsWorkQueue ? "owner-1" : "plain-1",
    isActive: true,
    status: "active",
    sessionType: "claude-cli",
    managedCodebase: false,
    ownsWorkQueue,
  });
}

function aSessionView(session: SessionEntry, withQueue = true) {
  const { backend } = aReplayBackend({
    counts: [1],
    snapshot: [replayAgentText("Reading the parser.", 1_000)],
  });
  const client = createClient(ActivityService, backend.transport());
  mountWithRpc(
    <div style={{ display: "flex", flexDirection: "column", height: 600, width: 1200 }}>
      <SessionWithActivitySidebar
        session={session}
        sessionToken="tok"
        client={client}
        suppressed={false}
        workQueuePanel={withQueue ? <div data-testid="work-queue-panel-content">queue</div> : undefined}
        toolbar={(toggle) => <div data-testid="session-toolbar">{toggle}</div>}
      >
        <div data-testid="session-main-content">terminal</div>
      </SessionWithActivitySidebar>
    </div>,
    backend,
  );
}

const queueSidebar = () => byTestId("work-queue-sidebar");
const queueButton = () => byTestId("work-queue-button");

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("a session that owns a queue opens on the work queue sidebar, to the right of the main content", () => {
  // When
  aSessionView(aSession(true));

  // Then
  queueSidebar().should("be.visible");
  byTestId("work-queue-panel-content").should("be.visible");
  byTestId("session-main-content").then(($main) =>
    queueSidebar().then(($side) => {
      expect($side[0].getBoundingClientRect().left).to.be.at.least($main[0].getBoundingClientRect().right);
    }),
  );
  agentActivityPage.sidebar().should("not.exist");
});

it("opening Activities closes the work queue, and opening the queue closes Activities", () => {
  // Given
  aSessionView(aSession(true));
  queueSidebar().should("be.visible");

  // When
  agentActivityPage.button().click();

  // Then
  agentActivityPage.sidebar().should("be.visible");
  queueSidebar().should("not.exist");

  // When
  queueButton().click();

  // Then
  queueSidebar().should("be.visible");
  agentActivityPage.sidebar().should("not.exist");
});

it("the toggle closes the work queue sidebar", () => {
  // Given
  aSessionView(aSession(true));
  queueSidebar().should("be.visible");

  // When
  queueButton().click();

  // Then
  queueSidebar().should("not.exist");
});

it("a session without a queue offers no work queue toggle", () => {
  // When
  aSessionView(aSession(false), false);

  // Then
  agentActivityPage.button().should("be.visible");
  queueButton().should("not.exist");
});
