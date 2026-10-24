/**
 * Acceptance: clicking a project's pinned **main** entry when the project has no home agent opens a
 * setup pane in the Sessions screen's detail area (`#/sessions/project-home/<projectId>`) instead of
 * sending the operator to the Projects screen. The pane offers the project's **Home agent** picker,
 * focused; choosing an agent stores it through `SetProjectHomeAgent` on the selected host, and
 * **Start main session** then ensures and selects the home session. The drawer shows no hint text.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { ProjectService, type ProjectEntry } from "../../src/gen/project_pb";
import { SessionEntrySchema, SessionService } from "../../src/gen/session_pb";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ProjectHomeSetupPane } from "../../src/components/sessions/ProjectHomeSetupPane";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";
import {
  byTestId,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectHome,
  sessionsProjectHomeSetup,
  sessionsProjectHomeSetupClose,
  sessionsProjectHomeSetupError,
  sessionsProjectHomeSetupStart,
  sessionsProjectHomeSetupUnknown,
} from "../support/testIds";

const BETA = "proj-beta";
const HOME_SESSION_ID = "home-beta-0000-0000-0000-000000000000";

/**
 * The Sessions screen over a daemon listing project `beta` with no home agent and one of its
 * sessions. `SetProjectHomeAgent` stores into the listed row, so the next `ListProjects` sees it;
 * `EnsureProjectHomeSession` answers with the home session.
 */
function theSessionsScreen() {
  const beta: Partial<ProjectEntry> = {
    projectId: BETA,
    name: "beta",
    mainBranchRef: "origin/master",
    homeAgent: "",
  };
  const backend = aSessionServiceBackend({
    sessions: [{ sessionId: "b1", projectId: BETA }],
    projectsOverride: [beta],
    agents: [{ id: "claude", label: "Claude" }],
  })
    .onUnary(ProjectService.method.setProjectHomeAgent, (req) => {
      beta.homeAgent = req.homeAgent;
      return {};
    })
    .onUnary(SessionService.method.ensureProjectHomeSession, () => ({
      session: create(SessionEntrySchema, {
        sessionId: HOME_SESSION_ID,
        projectId: BETA,
        sessionRole: "project-home",
      }),
      outcome: "started",
    }));
  mountWithRecordingLiveKitRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

function aPane(overrides: Partial<React.ComponentProps<typeof ProjectHomeSetupPane>> = {}) {
  cy.mount(
    <ProjectHomeSetupPane
      projectId={BETA}
      project={{ projectId: BETA, name: "beta", homeAgent: "claude", mainBranchRef: "origin/master" }}
      agents={[{ id: "claude", label: "Claude" }]}
      onSetHomeAgent={cy.stub().resolves()}
      onStart={cy.stub().resolves()}
      onClose={cy.stub()}
      {...overrides}
    />,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("clicking main on a project with no home agent opens its setup pane on the Sessions screen", () => {
  // Given
  theSessionsScreen();

  // When
  byTestId(sessionsDrawerProjectHome(BETA), { timeout: 5000 }).click();

  // Then
  cy.location("hash").should("match", new RegExp(`^#/sessions/project-home/${BETA}(\\?|$)`));
  byTestId(sessionsProjectHomeSetup(BETA)).should("contain", "Set up main for beta");
  projectsScreenPage.homeAgentSelect(BETA).should("have.focus");
  byTestId(sessionsProjectHomeSetupStart(BETA)).should("be.disabled");
});

it("shows no hint text under a main entry with no home agent", () => {
  // When
  theSessionsScreen();

  // Then
  byTestId(sessionsDrawerProjectHome(BETA), { timeout: 5000 }).should("exist");
  byTestId(sessionsDrawerProjectGroup(BETA)).should("not.contain", "Projects screen");
});

it("choosing a home agent in the pane stores it on the selected host and enables Start", () => {
  // Given
  const backend = theSessionsScreen();
  byTestId(sessionsDrawerProjectHome(BETA), { timeout: 5000 }).click();

  // When
  projectsScreenPage.homeAgentSelect(BETA).select("claude");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ProjectService.method.setProjectHomeAgent);
    expect(calls).to.have.length(1);
    expect(calls[0]).to.include({ projectId: BETA, homeAgent: "claude" });
  });
  byTestId(sessionsProjectHomeSetupStart(BETA)).should("not.be.disabled");
  byTestId(sessionsDrawerProjectHome(BETA)).should("contain", "Claude");
});

it("Start main session ensures the home session and selects it", () => {
  // Given
  const backend = theSessionsScreen();
  byTestId(sessionsDrawerProjectHome(BETA), { timeout: 5000 }).click();
  projectsScreenPage.homeAgentSelect(BETA).select("claude");

  // When
  byTestId(sessionsProjectHomeSetupStart(BETA)).should("not.be.disabled").click();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(SessionService.method.ensureProjectHomeSession);
    expect(calls).to.have.length(1);
    expect(calls[0]).to.include({ projectId: BETA });
  });
  cy.location("hash").should("match", new RegExp(`^#/sessions/${HOME_SESSION_ID}(\\?|$)`));
  byTestId(sessionsProjectHomeSetup(BETA)).should("not.exist");
});

it("Cancel closes the pane back to the drawer root", () => {
  // Given
  theSessionsScreen();
  byTestId(sessionsDrawerProjectHome(BETA), { timeout: 5000 }).click();

  // When
  byTestId(sessionsProjectHomeSetupClose(BETA)).click();

  // Then
  cy.location("hash").should("match", /^#\/sessions(\?|$)/);
  byTestId(sessionsProjectHomeSetup(BETA)).should("not.exist");
});

it("shows the daemon's reason when starting the main session is refused", () => {
  // Given
  aPane({ onStart: cy.stub().rejects(new Error("agent claude is not installed")) });

  // When
  byTestId(sessionsProjectHomeSetupStart(BETA)).click();

  // Then
  byTestId(sessionsProjectHomeSetupError(BETA)).should("contain", "agent claude is not installed");
});

it("says so when the host does not list the project, and offers nothing to start", () => {
  // When
  aPane({ project: null });

  // Then
  byTestId(sessionsProjectHomeSetupUnknown(BETA)).should("contain", BETA);
  byTestId(sessionsProjectHomeSetupStart(BETA)).should("be.disabled");
});
