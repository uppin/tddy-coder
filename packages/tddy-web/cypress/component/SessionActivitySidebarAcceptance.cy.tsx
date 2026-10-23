/**
 * Acceptance: the Agent Activity transcript is a right sidebar of the session view — open by
 * default for a managed-codebase session, closed until toggled for any other, and absent while
 * the main pane already shows the Activities view.
 *
 * Feature: docs/ft/web/agent-activity-pane.md § Right sidebar (`#project-home` 3/7).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { ActivityService } from "../../src/gen/activity_pb";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { SessionWithActivitySidebar } from "../../src/components/sessions/SessionWithActivitySidebar";
import { mountWithRpc } from "../support/rpc/inMemory";
import { agentActivityPage } from "../support/pages/agentActivityPage";
import { agentChatPage } from "../support/pages/agentChatPage";
import { byTestId } from "../support/testIds";
import { aReplayBackend, replayAgentText } from "../support/rpc/acpReplay";

function aSession(managedCodebase: boolean): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: managedCodebase ? "managed-1" : "plain-1",
    isActive: true,
    status: "active",
    sessionType: "claude-cli",
    managedCodebase,
  });
}

function aSessionView(session: SessionEntry, options: { suppressed?: boolean } = {}) {
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
        suppressed={options.suppressed ?? false}
        toolbar={(toggle) => <div data-testid="session-toolbar">{toggle}</div>}
      >
        <div data-testid="session-main-content">terminal</div>
      </SessionWithActivitySidebar>
    </div>,
    backend,
  );
}

/** The sidebar sits to the right of the main content. */
function sidebarIsRightOfMainContent() {
  byTestId("session-main-content").then(($main) => {
    agentActivityPage.sidebar().then(($side) => {
      expect($side[0].getBoundingClientRect().left).to.be.at.least($main[0].getBoundingClientRect().right);
    });
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("selecting a managed session shows the activity sidebar on the right", () => {
  // When
  aSessionView(aSession(true));

  // Then
  agentActivityPage.sidebar().should("be.visible");
  sidebarIsRightOfMainContent();
  agentChatPage.chatMessage(0).should("have.text", "Reading the parser.");
});

it("selecting an unmanaged session leaves the sidebar closed until toggled", () => {
  // Given
  aSessionView(aSession(false));
  agentActivityPage.button().should("be.visible");
  agentActivityPage.sidebar().should("not.exist");

  // When
  agentActivityPage.open();

  // Then
  agentActivityPage.sidebar().should("be.visible");
  sidebarIsRightOfMainContent();
});

it("no activity sidebar while the Activities view is the main pane", () => {
  // When
  aSessionView(aSession(true), { suppressed: true });

  // Then
  byTestId("session-main-content").should("be.visible");
  agentActivityPage.sidebar().should("not.exist");
});
