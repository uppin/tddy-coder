/**
 * Acceptance tests: the session detail pane (`SessionMainPane`) gains an "Add agent" button in its
 * header and a "Session agents" section listing the selected session's peers. Switching to a peer
 * from the section fires `onSwitchPeer` with the peer's session id (the parent wires this to
 * selecting the peer in the drawer, which focuses its runtime).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-07-27-session-agent.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import {
  SessionEntrySchema,
  ProjectEntrySchema,
  type SessionEntry,
  type ProjectEntry,
} from "../../src/gen/connection_pb";
import { SessionMainPane } from "../../src/components/sessions/SessionMainPane";
import type { SessionAttachmentState } from "../../src/components/sessions/useSessionAttachment";
import { sessionAgentsPage } from "../support/pages/sessionAgentsPage";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CURRENT_SESSION_ID = "session-current-aaaa-0000-0000-000000000001";
const PEER_CURSOR_ID = "peer-cursor-aaaa-0000-0000-000000000002";
const PEER_CLAUDE_ID = "peer-claude-aaaa-0000-0000-000000000003";

function aSession(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: CURRENT_SESSION_ID,
    createdAt: "2026-07-27T09:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 11111,
    isActive: true,
    projectId: "proj-1",
    pendingElicitation: false,
    orchestratorSessionId: "",
    agent: "claude",
    model: "sonnet-4",
    ...overrides,
  });
}

const CURRENT_SESSION = aSession({ sessionId: CURRENT_SESSION_ID });

const PEERS: SessionEntry[] = [
  aSession({
    sessionId: PEER_CURSOR_ID,
    orchestratorSessionId: CURRENT_SESSION_ID,
    agent: "cursor",
    model: "cursor-default",
  }),
  aSession({
    sessionId: PEER_CLAUDE_ID,
    orchestratorSessionId: CURRENT_SESSION_ID,
    agent: "claude",
    model: "opus-4",
  }),
];

const PROJECT: ProjectEntry = create(ProjectEntrySchema, {
  projectId: "proj-1",
  name: "project",
  gitUrl: "https://example.com/project.git",
  mainRepoPath: "/home/dev/project",
});

const aConnectedGrpcAttachment: SessionAttachmentState = {
  status: "connected-grpc",
  sessionId: CURRENT_SESSION_ID,
};

const noopHandlers = {
  inspectorState: "closed" as const,
  onToggleInspector: () => undefined,
  onInspectorClose: () => undefined,
  onInspectorExpand: () => undefined,
  onInspectorRestore: () => undefined,
  onResume: () => undefined,
  onDelete: () => undefined,
  onTerminate: () => undefined,
};

// ---------------------------------------------------------------------------
// Fluent driver
// ---------------------------------------------------------------------------

interface MainPaneOptions {
  sessions?: SessionEntry[];
  onSwitchPeer?: (sessionId: string) => void;
}

function aSessionMainPane(options: MainPaneOptions = {}) {
  const onSwitchPeer = options.onSwitchPeer ?? cy.stub().as("onSwitchPeer");
  const sessions = options.sessions ?? PEERS;
  const driver = {
    mount() {
      cy.mount(
        <SessionMainPane
          {...noopHandlers}
          selectedSession={CURRENT_SESSION}
          attachment={aConnectedGrpcAttachment}
          projects={[PROJECT]}
          sessions={sessions}
          runtimes={[]}
          focusedRuntimeId={null}
          onSwitchPeer={onSwitchPeer}
        />,
      );
      return driver;
    },
    addAgentBtn: () => sessionAgentsPage.addAgentBtn(),
    section: () => sessionAgentsPage.section(),
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
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("renders the Add agent button in the session-detail header", () => {
  // Given / When
  aSessionMainPane().mount();

  // Then
  sessionAgentsPage.addAgentBtn().should("be.visible");
  sessionsDrawerPage.detailPane().should("exist");
});

it("lists the selected session's peers in the Session agents section", () => {
  // Given / When
  aSessionMainPane().mount();

  // Then
  sessionAgentsPage.peerRowSessionIds().should("deep.equal", [PEER_CURSOR_ID, PEER_CLAUDE_ID]);
});

it("renders the empty state when the selected session has no peers", () => {
  // Given — no peers in the sessions list
  // When
  aSessionMainPane({ sessions: [CURRENT_SESSION] }).mount();

  // Then — the empty-state message is shown (the peers list section is not rendered at all)
  sessionAgentsPage.emptyState().should("be.visible");
});

it("fires onSwitchPeer with the peer's session id when its switch button is clicked", () => {
  // Given
  const onSwitchPeer = cy.stub().as("onSwitchPeer");
  const pane = aSessionMainPane({ onSwitchPeer });

  // When
  pane.mount();
  pane.clickSwitch(PEER_CURSOR_ID);

  // Then
  cy.get("@onSwitchPeer").should("have.been.calledOnceWith", PEER_CURSOR_ID);
});
