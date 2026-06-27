/**
 * Acceptance tests for the Create New Session flow in the sessions drawer.
 *
 * All tests mount SessionsDrawerScreen and exercise the full flow via intercepted RPCs.
 */
import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import {
  interceptConnectionRpcs,
  interceptConnectSession,
  interceptListProjectBranches,
  interceptStartSession,
} from "../support/rpc/connectionRpcs";
import { TEST_IDS, byTestId } from "../support/testIds";
import { CLAUDE_CLI_MODELS } from "../../src/constants/claudeCliModels";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const CONNECTED_SESSION = {
  sessionId: "create-connected-aaaa-0000-0000-0000-000000000000",
  createdAt: "2026-06-25T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/my-feature",
  pid: 10001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
  workflowGoal: "Existing work",
  pendingElicitation: false,
};

const NEW_SESSION_ID = "new-session-bbbb-0000-0000-0000-000000000001";

/** Fixture returned by the second listSessions call after creation. */
const NEW_SESSION_FIXTURE = {
  sessionId: NEW_SESSION_ID,
  createdAt: "2026-06-26T12:10:00Z",
  status: "active",
  repoPath: "/home/dev/new-feature",
  pid: 20001,
  isActive: true,
  projectId: "proj-1",
  daemonInstanceId: "",
  workflowGoal: "New work",
  pendingElicitation: false,
};

// ---------------------------------------------------------------------------

describe("CreateSession acceptance — button, form, and post-create navigation", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC1: New session button is visible in the drawer header
  // -------------------------------------------------------------------------

  it("shows a '+ New session' button in the sessions drawer header", () => {
    interceptConnectionRpcs([CONNECTED_SESSION]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawer).within(() => {
      byTestId(TEST_IDS.sessionsDrawerNewBtn).should("be.visible");
    });
  });

  // -------------------------------------------------------------------------
  // AC2: Clicking the button shows the create form in the main pane
  // -------------------------------------------------------------------------

  it("clicking '+ New session' shows the create form in the main pane with the drawer still visible", () => {
    interceptConnectionRpcs([CONNECTED_SESSION]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();

    // Create pane appears in main area
    byTestId(TEST_IDS.createSessionPane).should("be.visible");
    // Drawer remains visible
    byTestId(TEST_IDS.sessionsDrawer).should("be.visible");
  });

  // -------------------------------------------------------------------------
  // AC3: Session type toggle switches fields
  // -------------------------------------------------------------------------

  it("switching to Claude CLI hides Agent/Recipe and shows Model/Permission/Prompt", () => {
    interceptConnectionRpcs([]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();

    // By default: Tool fields visible
    byTestId(TEST_IDS.createSessionAgentSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionRecipeSelect).should("be.visible");

    // Switch to Claude CLI
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    // Tool fields hidden
    byTestId(TEST_IDS.createSessionAgentSelect).should("not.exist");
    byTestId(TEST_IDS.createSessionRecipeSelect).should("not.exist");

    // Claude CLI fields visible
    byTestId(TEST_IDS.createSessionModelSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionPermissionModeSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionInitialPromptInput).should("be.visible");

    // Switch back to Tool
    byTestId(TEST_IDS.createSessionTypeToolBtn).click();

    byTestId(TEST_IDS.createSessionAgentSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionModelSelect).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC4: Project dropdown populated from ListProjects RPC
  // -------------------------------------------------------------------------

  it("populates the project dropdown from the ListProjects RPC response", () => {
    interceptConnectionRpcs([], {
      projectsOverride: [
        { projectId: "proj-alpha", name: "Alpha Project", mainRepoPath: "/home/dev/alpha" },
        { projectId: "proj-beta", name: "Beta Project", mainRepoPath: "/home/dev/beta" },
      ],
    });

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");

    byTestId(TEST_IDS.createSessionProjectSelect).within(() => {
      cy.get("option").should("contain.text", "Alpha Project");
      cy.get("option").should("contain.text", "Beta Project");
    });
  });

  // -------------------------------------------------------------------------
  // AC5: Agent dropdown populated from ListAgents RPC (tool session)
  // -------------------------------------------------------------------------

  it("populates the agent dropdown from the ListAgents RPC response", () => {
    interceptConnectionRpcs([], {
      agents: [
        { id: "claude", label: "Claude (opus)" },
        { id: "codex", label: "Codex" },
      ],
    });

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listAgents");

    byTestId(TEST_IDS.createSessionAgentSelect).within(() => {
      cy.get("option").should("contain.text", "Claude (opus)");
      cy.get("option").should("contain.text", "Codex");
    });
  });

  // -------------------------------------------------------------------------
  // AC6: Model dropdown shows CLAUDE_CLI_MODELS (claude-cli session)
  // -------------------------------------------------------------------------

  it("shows all CLAUDE_CLI_MODELS in the model dropdown when session type is Claude CLI", () => {
    interceptConnectionRpcs([]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    byTestId(TEST_IDS.createSessionTypeClaudeCliBtn).click();

    byTestId(TEST_IDS.createSessionModelSelect).within(() => {
      CLAUDE_CLI_MODELS.forEach((m) => {
        cy.get("option").should("contain.text", m.label);
      });
    });
  });

  // -------------------------------------------------------------------------
  // AC7: Create button disabled until required fields are filled
  // -------------------------------------------------------------------------

  it("Create button is disabled until required fields are filled (tool session)", () => {
    interceptConnectionRpcs([], {
      projectsOverride: [],
    });

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");

    // No projects available → create button disabled
    byTestId(TEST_IDS.createSessionSubmitBtn).should("be.disabled");
  });

  // -------------------------------------------------------------------------
  // AC8: Branch intent — new branch shows name input
  // -------------------------------------------------------------------------

  it("shows the new branch name input when branch mode is 'new branch from base'", () => {
    interceptConnectionRpcs([]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();

    byTestId(TEST_IDS.createSessionBranchIntentSelect)
      .select("new_branch_from_base");

    byTestId(TEST_IDS.createSessionNewBranchNameInput).should("be.visible");
    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC9: Branch intent — existing branch shows branch selector
  // -------------------------------------------------------------------------

  it("shows a branch selector when branch mode is 'work on existing branch'", () => {
    interceptConnectionRpcs([], {
      projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
    });
    interceptListProjectBranches(["origin/main", "origin/feature-x"]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionBranchIntentSelect).select("work_on_selected_branch");

    cy.wait("@listProjectBranches");

    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).should("be.visible");
    byTestId(TEST_IDS.createSessionBranchToWorkOnSelect).within(() => {
      cy.get("option").should("contain.text", "origin/main");
      cy.get("option").should("contain.text", "origin/feature-x");
    });
    byTestId(TEST_IDS.createSessionNewBranchNameInput).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC10: Cancel returns to the session list placeholder
  // -------------------------------------------------------------------------

  it("clicking Cancel dismisses the create form and restores the main pane placeholder", () => {
    interceptConnectionRpcs([]);

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    byTestId(TEST_IDS.createSessionPane).should("be.visible");

    byTestId(TEST_IDS.createSessionCancelBtn).click();

    byTestId(TEST_IDS.createSessionPane).should("not.exist");
    byTestId(TEST_IDS.sessionsDetailPane).should("be.visible");
  });

  // -------------------------------------------------------------------------
  // AC11: Successful creation navigates to the new session and auto-attaches
  // -------------------------------------------------------------------------

  it("submitting the form calls StartSession and auto-attaches to the new session", () => {
    interceptConnectionRpcs([], {
      projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
      agents: [{ id: "claude", label: "Claude (opus)" }],
    });
    interceptStartSession(NEW_SESSION_ID);
    interceptConnectSession({ livekitRoom: `room-${NEW_SESSION_ID}` });

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");
    cy.wait("@listAgents");

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");

    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();

    cy.wait("@startSession");
    cy.wait("@connectSession");

    // Form dismissed after success
    byTestId(TEST_IDS.createSessionPane).should("not.exist");

    // The new session's URL segment is reflected in the hash
    cy.window().its("location.hash").should("include", NEW_SESSION_ID);
  });

  // -------------------------------------------------------------------------
  // AC12: Error on StartSession failure shows error message
  // -------------------------------------------------------------------------

  it("shows an error message when StartSession RPC fails and keeps the form open", () => {
    interceptConnectionRpcs([], {
      projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
      agents: [{ id: "claude", label: "Claude (opus)" }],
    });
    cy.intercept("POST", "**/rpc/connection.ConnectionService/StartSession", (req) => {
      req.reply({ statusCode: 500, body: "internal error" });
    }).as("startSessionFail");

    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");
    cy.wait("@listAgents");

    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");

    byTestId(TEST_IDS.createSessionSubmitBtn).click();
    cy.wait("@startSessionFail");

    byTestId(TEST_IDS.createSessionError).should("be.visible");
    // Form remains open
    byTestId(TEST_IDS.createSessionPane).should("be.visible");
  });
});

// ---------------------------------------------------------------------------
// AC13-14: Post-creation list refresh
// Bug: after handleSessionCreated(), sessions state is never re-fetched, so the
// new session is absent from sortedSessions → selectedSession is null → empty pane.
// ---------------------------------------------------------------------------

describe("CreateSession acceptance — post-creation list refresh", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    window.localStorage.setItem("tddy_session_token", "fake-token");
  });

  // -------------------------------------------------------------------------
  // AC13: After creation the sessions list is re-fetched (second ListSessions call)
  //       and the new session appears as a drawer item.
  // -------------------------------------------------------------------------

  it("re-fetches the sessions list after creation so the new session appears in the drawer", () => {
    // Given — first listSessions returns empty; second returns the newly-created session
    let callCount = 0;
    interceptConnectionRpcs([], {
      projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
      agents: [{ id: "claude", label: "Claude (opus)" }],
      listSessionsFactory: () => {
        callCount++;
        return callCount > 1 ? [NEW_SESSION_FIXTURE] : [];
      },
    });
    interceptStartSession(NEW_SESSION_ID);
    interceptConnectSession({ livekitRoom: `room-${NEW_SESSION_ID}` });

    // When — mount, create a session
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions"); // 1st call on mount

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");
    cy.wait("@listAgents");
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();
    cy.wait("@startSession");

    // Then — a second listSessions call must be made after creation (fix triggers this)
    cy.wait("@listSessions"); // 2nd call — times out without the fix

    // And the new session appears in the drawer
    sessionsDrawerPage.drawerItem(NEW_SESSION_ID).should("exist");
  });

  // -------------------------------------------------------------------------
  // AC14: After creation the detail pane shows the new session's terminal,
  //       not the "Select a session" empty placeholder.
  // -------------------------------------------------------------------------

  it("shows the new session's terminal in the detail pane rather than the empty placeholder", () => {
    // Given — same two-phase list setup
    let callCount = 0;
    interceptConnectionRpcs([], {
      projectsOverride: [{ projectId: "proj-1", name: "Test Project" }],
      agents: [{ id: "claude", label: "Claude (opus)" }],
      listSessionsFactory: () => {
        callCount++;
        return callCount > 1 ? [NEW_SESSION_FIXTURE] : [];
      },
    });
    interceptStartSession(NEW_SESSION_ID);
    interceptConnectSession({ livekitRoom: `room-${NEW_SESSION_ID}` });

    // When
    cy.mount(<SessionsDrawerScreen />);
    cy.wait("@listSessions");

    byTestId(TEST_IDS.sessionsDrawerNewBtn).click();
    cy.wait("@listProjects");
    cy.wait("@listAgents");
    byTestId(TEST_IDS.createSessionProjectSelect).select("proj-1");
    byTestId(TEST_IDS.createSessionAgentSelect).select("claude");
    byTestId(TEST_IDS.createSessionSubmitBtn).should("not.be.disabled").click();
    cy.wait("@startSession");
    cy.wait("@listSessions"); // wait for post-creation re-fetch

    // Then — empty placeholder is gone
    cy.contains("Select a session").should("not.exist");

    // And the terminal container is visible (session was auto-connected)
    sessionsDrawerPage.detailTerminalContainer().should("exist");
  });
});
