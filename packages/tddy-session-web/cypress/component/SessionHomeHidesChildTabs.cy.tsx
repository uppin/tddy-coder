/**
 * Cypress component acceptance: a project-home or orchestrator session hides child tabs.
 *
 * PRD: `docs/ft/coder/spawn-conversation.md`
 *
 * Since `#start-parity`, every session a home/orchestrator starts through `StartSession` records
 * `orchestrator_session_id` = that session — so the child-tab derivation would turn each one into
 * a "conversation" tab. Those are started sessions, not spawned conversations, and most have ended:
 * selecting one attaches, the stream ends, and the Agent pane comes back. Home and orchestrator
 * sessions therefore show no child tabs. An ordinary session that spawned a child keeps its tab.
 *
 * Driven over the deterministic host-served path: `connectSession` returns an empty `livekitRoom`,
 * so the parent is reached over its host's own connection and its RPCs flow into the in-memory
 * backend.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  aSessionServiceBackend,
  type SessionServiceBackend,
} from "../support/rpc/daemonSessionHostBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { sessionTerminalTabsPage as tabs } from "../support/pages/sessionTerminalTabsPage";

// ---------------------------------------------------------------------------
// Fixtures — a project-home parent, an orchestrator parent, and started children
// ---------------------------------------------------------------------------

// `sessionType: "cursor-cli"` is a genuine PTY terminal, so it renders the terminal runtime that
// hosts the tab bar — unlike a `tool` workflow session, which opens the full-screen chat instead
// (WorkflowChatScreen) and has no terminal tab bar. Leaving `sessionType` unset would proto-default
// to "" (tool) and route to chat, so it must be set explicitly.
const HOME_PARENT = {
  sessionId: "home-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-19T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 91001,
  isActive: true,
  projectId: "proj-home-1",
  daemonInstanceId: "local",
  sessionType: "cursor-cli",
  sessionRole: "project-home",
  pendingElicitation: false,
};

const HOME_CHILD = {
  sessionId: "home-tabs-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-19T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-alpha",
  pid: 91002,
  isActive: true,
  projectId: "proj-home-1",
  daemonInstanceId: "local",
  orchestratorSessionId: HOME_PARENT.sessionId,
  pendingElicitation: false,
};

const ORCHESTRATOR_PARENT = {
  sessionId: "orch-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-19T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-beta",
  pid: 92001,
  isActive: true,
  projectId: "proj-orch-1",
  daemonInstanceId: "local",
  sessionType: "cursor-cli",
  sessionRole: "orchestrator",
  pendingElicitation: false,
};

const ORCHESTRATOR_CHILD = {
  sessionId: "orch-tabs-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-19T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-beta",
  pid: 92002,
  isActive: true,
  projectId: "proj-orch-1",
  daemonInstanceId: "local",
  orchestratorSessionId: ORCHESTRATOR_PARENT.sessionId,
  pendingElicitation: false,
};

const ORDINARY_PARENT = {
  sessionId: "ordinary-tabs-aaaaaaaa-0000-0000-0000-000000000001",
  createdAt: "2026-07-19T09:00:00Z",
  status: "active",
  repoPath: "/home/dev/feature-gamma",
  pid: 93001,
  isActive: true,
  projectId: "proj-ordinary-1",
  daemonInstanceId: "local",
  recipe: "grill-me",
  sessionType: "cursor-cli",
  pendingElicitation: false,
};

const ORDINARY_CHILD = {
  sessionId: "ordinary-tabs-bbbbbbbb-0000-0000-0000-000000000002",
  createdAt: "2026-07-19T09:05:00Z",
  status: "active",
  repoPath: "/home/dev/feature-gamma/.worktrees/implement-plan",
  pid: 93002,
  isActive: true,
  projectId: "proj-ordinary-1",
  daemonInstanceId: "local",
  orchestratorSessionId: ORDINARY_PARENT.sessionId,
  pendingElicitation: false,
};

/** A host-served backend (empty `livekitRoom`) listing the given sessions. */
function aGrpcBackend(
  sessions: Array<Record<string, unknown>>,
): SessionServiceBackend {
  return aSessionServiceBackend({
    sessions,
    connectSession: () => ({ livekitRoom: "", livekitUrl: "", livekitServerIdentity: "" }),
  });
}

/** Attach the given session over gRPC and wait for its terminal tab bar to render. */
function attachSession(backend: SessionServiceBackend, sessionId: string) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(sessionId).click();
  tabs.tabs().should("exist");
}

// ---------------------------------------------------------------------------

describe("SessionHomeHidesChildTabs — home/orchestrator sessions show no child tabs", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  it("shows only the Agent tab for a project-home session with started sessions", () => {
    // Given a project-home session that has started other sessions
    const backend = aGrpcBackend([HOME_PARENT, HOME_CHILD]);

    // When the home session is attached
    attachSession(backend, HOME_PARENT.sessionId);

    // Then the Agent tab is present and no child-conversation tabs are rendered
    tabs.agentTab().should("exist");
    tabs.childTabs().should("not.exist");
  });

  it("shows only the Agent tab for an orchestrator session with started sessions", () => {
    // Given an orchestrator session that has started other sessions
    const backend = aGrpcBackend([ORCHESTRATOR_PARENT, ORCHESTRATOR_CHILD]);

    // When the orchestrator session is attached
    attachSession(backend, ORCHESTRATOR_PARENT.sessionId);

    // Then the Agent tab is present and no child-conversation tabs are rendered
    tabs.agentTab().should("exist");
    tabs.childTabs().should("not.exist");
  });

  it("still shows the spawned child tab for an ordinary session", () => {
    // Given an ordinary session that has spawned a child conversation
    const backend = aGrpcBackend([ORDINARY_PARENT, ORDINARY_CHILD]);

    // When the ordinary session is attached
    attachSession(backend, ORDINARY_PARENT.sessionId);

    // Then the parent's tab bar shows the fixed Agent tab and a tab for the spawned child
    tabs.agentTab().should("exist").and("have.attr", "aria-selected", "true");
    tabs.childTab(ORDINARY_CHILD.sessionId).should("exist");
  });
});
