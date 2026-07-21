/**
 * Cypress component acceptance: spawned child conversations render as tabs in the parent session.
 *
 * PRD: `docs/ft/coder/spawn-conversation.md`
 * Changeset: `spawn-conversation`
 *
 * When a workflow (e.g. grill-me) calls `spawn_conversation`, the daemon creates a child session on
 * a new worktree tagged with `orchestratorSessionId = <parent>`. The web discovers that child via
 * the ordinary `ListSessions` poll and renders it as a tab inside the parent's session runtime.
 *
 * Driven over the deterministic gRPC path: `connectSession` returns an empty `livekitRoom`, so both
 * the parent and a selected child attach as `connected-grpc` and their RPCs flow over the daemon
 * client into the in-memory backend.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aConnectionServiceBackend,
  type ConnectionServiceBackend,
} from "../support/rpc/connectionServiceBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixtures — a grill-me parent session and a spawned child conversation
// ---------------------------------------------------------------------------

// A cursor-cli grill-me session: this is the real-world flow that spawns child conversations
// (grill-me on Cursor relays `spawn_conversation` to the daemon). `sessionType: "cursor-cli"` is a
// genuine PTY terminal, so it renders the terminal runtime that hosts the child-conversation tabs —
// unlike a `tool` workflow session, which now opens the full-screen chat instead (WorkflowChatScreen)
// and has no terminal tab bar. Leaving `sessionType` unset here would proto-default to "" (tool) and
// route to chat, so it must be set explicitly.
const PARENT = {
  sessionId: "child-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-19T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 90001,
  isActive: true,
  projectId: "proj-child-1",
  daemonInstanceId: "local",
  recipe: "grill-me",
  sessionType: "cursor-cli",
  pendingElicitation: false,
};

const CHILD = {
  sessionId: "child-tabs-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-19T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha/.worktrees/implement-plan",
  pid: 90002,
  isActive: true,
  projectId: "proj-child-1",
  daemonInstanceId: "local",
  orchestratorSessionId: PARENT.sessionId,
  pendingElicitation: false,
};

/** A connected-grpc backend (empty `livekitRoom`) listing the given sessions. */
function aGrpcBackend(sessions: typeof PARENT[] | Array<Record<string, unknown>>): ConnectionServiceBackend {
  return aConnectionServiceBackend({
    sessions,
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
  });
}

/** Attach the parent session over gRPC and wait for its terminal tab bar to render. */
function attachParent(backend: ConnectionServiceBackend) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PARENT.sessionId).click();
  tabs.tabs().should("exist");
}

// ---------------------------------------------------------------------------

describe("SessionChildTabs — spawned conversations render as tabs in the parent", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("renders a spawned child conversation as a tab inside the parent session runtime", () => {
    // Given a grill-me session that has spawned one child conversation
    const backend = aGrpcBackend([PARENT, CHILD]);

    // When the parent session is attached
    attachParent(backend);

    // Then the parent's tab bar shows the fixed Agent tab and a tab for the spawned child
    tabs.agentTab().should("exist").and("have.attr", "aria-selected", "true");
    tabs.childTab(CHILD.sessionId).should("exist");
  });

  it("selecting the child conversation tab attaches the child session and shows its pane", () => {
    // Given an attached grill-me parent with one spawned child conversation
    const backend = aGrpcBackend([PARENT, CHILD]);
    attachParent(backend);

    // When the child conversation tab is selected
    tabs.childTab(CHILD.sessionId).click();

    // Then the child session is attached (ConnectSession called for the child's id) ...
    cy.wrap(backend).should((b: ConnectionServiceBackend) => {
      expect(b.connectedSessionIds).to.include(CHILD.sessionId);
    });

    // ... and the child's runtime pane is shown while its tab becomes selected.
    tabs.childTab(CHILD.sessionId).should("have.attr", "aria-selected", "true");
    tabs.childPane(CHILD.sessionId).should("be.visible");
    tabs.agentTab().should("have.attr", "aria-selected", "false");
  });

  it("shows only the Agent tab when the grill-me session has spawned no children", () => {
    // Given a grill-me session with no spawned child conversations
    const backend = aGrpcBackend([PARENT]);

    // When it is attached
    attachParent(backend);

    // Then the Agent tab is present and no child-conversation tabs are rendered
    tabs.agentTab().should("exist");
    tabs.childTabs().should("not.exist");
  });
});
