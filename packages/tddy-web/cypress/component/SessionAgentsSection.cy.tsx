/**
 * Acceptance tests: the "Session agents" section lists the current session's peer agent sessions
 * (children via `orchestratorSessionId`), each showing agent/model/status, with a switch action
 * that focuses the peer's runtime. Shows an empty state when there are no peers.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-07-27-session-agent.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/connection_pb";
import { SessionAgentsSection } from "../../src/components/sessions/SessionAgentsSection";
import { sessionAgentsPage } from "../support/pages/sessionAgentsPage";

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

const CURRENT_SESSION_ID = "session-current-aaaa-0000-0000-000000000001";

function aPeer(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "peer-default-aaaa-0000-0000-000000000010",
    createdAt: "2026-07-27T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 22222,
    isActive: true,
    projectId: "proj-1",
    orchestratorSessionId: CURRENT_SESSION_ID,
    agent: "cursor",
    model: "cursor-default",
    pendingElicitation: false,
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

interface SectionOptions {
  onSwitchPeer?: (sessionId: string) => void;
}

function aSessionAgentsSection(peers: SessionEntry[], options: SectionOptions = {}) {
  const onSwitchPeer = options.onSwitchPeer ?? cy.stub().as("onSwitchPeer");
  const driver = {
    mount() {
      cy.mount(
        <SessionAgentsSection
          peers={peers}
          onSwitchPeer={onSwitchPeer}
        />,
      );
      return driver;
    },
    section: () => sessionAgentsPage.section(),
    emptyState: () => sessionAgentsPage.emptyState(),
    peerRow: (sessionId: string) => sessionAgentsPage.peerRow(sessionId),
    peerSwitchBtn: (sessionId: string) => sessionAgentsPage.peerSwitchBtn(sessionId),
    peerRowSessionIds: () => sessionAgentsPage.peerRowSessionIds(),
    clickSwitch: (sessionId: string) => {
      sessionAgentsPage.peerSwitchBtn(sessionId).click();
      return driver;
    },
  };
  return driver;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("renders the empty state when the peer list is empty", () => {
  // Given
  const section = aSessionAgentsSection([]);

  // When
  section.mount();

  // Then — the empty-state message is shown (the peers list section is not rendered at all)
  section.emptyState().should("be.visible");
});

it("renders one row per peer with the peer's session id", () => {
  // Given
  const peers = [
    aPeer({ sessionId: "peer-cursor-1", agent: "cursor", model: "cursor-default" }),
    aPeer({ sessionId: "peer-claude-2", agent: "claude", model: "sonnet-4" }),
  ];
  const section = aSessionAgentsSection(peers);

  // When
  section.mount();

  // Then
  section.peerRowSessionIds().should("deep.equal", ["peer-cursor-1", "peer-claude-2"]);
});

it("fires onSwitchPeer with the peer's session id when its switch button is clicked", () => {
  // Given
  const peers = [aPeer({ sessionId: "peer-cursor-1" })];
  const onSwitchPeer = cy.stub().as("onSwitchPeer");
  const section = aSessionAgentsSection(peers, { onSwitchPeer });

  // When
  section.mount();
  section.clickSwitch("peer-cursor-1");

  // Then
  cy.get("@onSwitchPeer").should("have.been.calledOnceWith", "peer-cursor-1");
});

it("renders a peer with empty agent and model fields without error", () => {
  // Given — a peer whose agent/model were never populated (e.g. a tool session)
  const peers = [aPeer({ sessionId: "peer-bare", agent: "", model: "" })];
  const section = aSessionAgentsSection(peers);

  // When
  section.mount();

  // Then
  section.peerRow("peer-bare").should("be.visible");
  section.peerRowSessionIds().should("deep.equal", ["peer-bare"]);
});
