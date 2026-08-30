/**
 * Acceptance: the peer agent sessions an operator used to find in the session detail pane now live
 * in the Inspector's **Agents** tab, as branches of the main agent's tree — with an inferred status
 * instead of the session's lifecycle string, and with the Switch that focuses them.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-30-agents-tab-subagent-tree.md (AC18, AC23)
 * Changeset: docs/dev/1-WIP/CS-2026-08-30-agents-tab-subagent-tree.md
 *
 * Driven through `SessionsDrawerScreen` rather than through the pane, because the two properties
 * under test span three collaborators a narrower mount lets disagree in silence: the session list is
 * the drawer's, the removed section was `SessionMainPane`'s, and the tree is the Inspector's. It is
 * also the only level at which "Switch focuses that session" is observable at all — the pane can
 * report the id, but only the screen can select it.
 *
 * This spec replaces `SessionMainPanePeerSwitch.cy.tsx` and `SessionAgentsSection.cy.tsx`: their
 * subject was a section this change deletes.
 *
 * The session attaches over the deterministic gRPC path (`connectSession` returns an empty
 * `livekitRoom`), so every RPC lands in the in-memory backend — as in
 * `SessionAgentAttachTabAcceptance.cy.tsx`.
 */

import React from "react";
import { SessionAgentStatus } from "../../src/gen/connection_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { anActivity, anAttachedAgent } from "../support/rpc/sessionAgentRosterBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST = "local";
const MAIN_SESSION_ID = "tree-main-aaaaaaaa-0000-0000-0000-000000000001";
const CLAUDE_CHILD = "tree-child-aaaaaaaa-0000-0000-0000-000000000002";
const EXPLORER = "explorer@local";

const NOW_MS = 1_780_828_020_298;

const MAIN_SESSION = {
  sessionId: MAIN_SESSION_ID,
  createdAt: "2026-08-30T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-tree-1",
  daemonInstanceId: HOST,
  sessionType: "cursor-cli",
  agent: "cursor",
  model: "cursor-default",
  pendingElicitation: false,
};

/** A claude-cli session the main agent spawned, mid tool call as the daemon inferred it. */
const CLAUDE_SUBAGENT = {
  ...MAIN_SESSION,
  sessionId: CLAUDE_CHILD,
  orchestratorSessionId: MAIN_SESSION_ID,
  sessionType: "claude-cli",
  agent: "claude",
  model: "opus-4",
  agentStatus: SessionAgentStatus.EXECUTING_TOOL,
  lastActivity: anActivity("Bash: cargo test", NOW_MS - 12_000),
};

function aBackendWithASubagent(): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions: [MAIN_SESSION, CLAUDE_SUBAGENT],
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
    sessionAgents: {
      sessionId: MAIN_SESSION_ID,
      initial: [anAttachedAgent(EXPLORER)],
      rev: 1,
    },
  });
}

/** Select the main session and open its Agents tab — every case starts here. */
function openAgentsTab(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(MAIN_SESSION_ID).click();
  sessionsDrawerPage.inspectorToggle().click();
  page.openInspectorAgentsTab();
}

// ---------------------------------------------------------------------------

describe("Agents tab — the session's subagents, reached through the drawer", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows the session's spawned subagent under its main agent", () => {
    // Given a session with one attached agent and one spawned subagent
    openAgentsTab(aBackendWithASubagent());

    // Then both hang off the main agent, from the two feeds that reported them
    page.assertRosterAgentUnderMainAgent(EXPLORER);
    page.assertSubagentUnderMainAgent(CLAUDE_CHILD);
  });

  it("shows a subagent's inferred status rather than its session lifecycle string", () => {
    // Given a subagent the daemon has observed inside a tool call. Its `SessionEntry.status` is
    // "active", which is what the removed section showed and which says nothing about the agent.
    openAgentsTab(aBackendWithASubagent());

    // Then
    page.assertSubagentStatus(CLAUDE_CHILD, "executing-tool");
    page.subagentLastActivity(CLAUDE_CHILD).should("contain.text", "Bash: cargo test");
  });

  it("focuses the subagent's own session when its Switch is clicked", () => {
    // Given the main session is the one selected
    openAgentsTab(aBackendWithASubagent());
    sessionsDrawerPage.expectSessionSelected(MAIN_SESSION_ID);

    // When
    page.clickSwitch(CLAUDE_CHILD);

    // Then the drawer selects the subagent — the focus move the old section's Switch performed
    sessionsDrawerPage.expectSessionSelected(CLAUDE_CHILD);
  });

  it("no longer carries the Session agents section in the session detail pane", () => {
    // Given a session with a subagent — the exact case the section existed to list
    mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aBackendWithASubagent());
    sessionsDrawerPage.drawerItem(MAIN_SESSION_ID).click();

    // Then the peer list is the Agents tab's, and nothing lists it twice
    page.assertNoLegacyPeerSection();
  });
});
