/**
 * Reproduction: clicking a project's pinned **main** entry when the project has no home agent does
 * nothing.
 *
 * The entry renders `aria-disabled` and returns early from its click, and its only explanation is a
 * small grey hint under it. An operator who clicks "main" sees no response at all — no request, no
 * navigation, no error — and reads the drawer as broken. Expected: the click asks to set a home agent
 * up for that project (the Sessions screen opens its setup pane — see
 * `ProjectHomeSetupPaneAcceptance`). A project that has a home agent still starts its main session as
 * before. The Projects screen still focuses a project's picker when its address names the project.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { SessionDrawer, type ProjectHomeInfo } from "../../src/components/sessions/SessionDrawer";
import { AgentInfoSchema, CatalogService } from "../../src/gen/catalog_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aProject, aProjectsBackend, projectsAppPage } from "../support/rpc/projectsAppPageFixtures";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";
import { byTestId, sessionsDrawerProjectHome } from "../support/testIds";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };
const ALPHA_HOME: ProjectHomeInfo = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeAgentLabel: "Claude" };
const BETA_NO_AGENT: ProjectHomeInfo = { projectId: BETA.projectId, mainBranchRef: "origin/master", homeAgentLabel: null };

function aSession(sessionId: string, projectId: string): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    projectId,
    createdAt: "2026-10-22T10:00:00Z",
    status: "active",
    isActive: true,
    pid: 4242,
  });
}

function aDrawer(handlers: {
  onEnsure: (projectId: string) => Promise<void>;
  onSetUpProjectHome: (projectId: string) => void;
}) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={[aSession("a1", ALPHA.projectId), aSession("b1", BETA.projectId)]}
        projects={[ALPHA, BETA]}
        projectHomes={[ALPHA_HOME, BETA_NO_AGENT]}
        onEnsureProjectHome={handlers.onEnsure}
        onSetUpProjectHome={handlers.onSetUpProjectHome}
        selectedSessionId={null}
        onSelectSession={cy.stub()}
        isOpen
        onClose={cy.stub()}
        onOpen={cy.stub()}
        crossHostSessionsVisible
      />
    </TooltipProvider>,
  );
}

/** The Projects page listing ALPHA and BETA, neither with a home agent, and one catalog agent to pick. */
function theProjectsPage() {
  const projects = [ALPHA, BETA].map(({ projectId, name }) =>
    aProject({
      projectId,
      name,
      gitUrl: `https://example.com/${name}.git`,
      mainRepoPath: `/home/dev/repos/${name}`,
      homeAgent: "",
    }),
  );
  const backend = aProjectsBackend((b) =>
    b
      .onUnary(ProjectService.method.listProjects, () => ({ projects }))
      .onUnary(CatalogService.method.listAgents, () => ({
        agents: [create(AgentInfoSchema, { id: "claude", label: "Claude" })],
      })),
  );
  mountWithRpc(projectsAppPage(), backend);
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "";
});

it("clicking main on a project with no home agent asks to set one up for that project", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  const onSetUpProjectHome = cy.stub().as("onSetUpProjectHome");
  aDrawer({ onEnsure, onSetUpProjectHome });

  // When
  byTestId(sessionsDrawerProjectHome(BETA.projectId)).click();

  // Then the operator is sent to set the home agent, and nothing is started
  cy.get("@onSetUpProjectHome").should("have.been.calledOnceWith", BETA.projectId);
  cy.get("@onEnsure").should("not.have.been.called");
});

it("offers the main entry of a project with no home agent as an action, not as disabled", () => {
  // Given
  aDrawer({ onEnsure: cy.stub().resolves(), onSetUpProjectHome: cy.stub() });

  // Then
  byTestId(sessionsDrawerProjectHome(BETA.projectId)).should("not.have.attr", "aria-disabled", "true");
});

it("clicking main on a project with a home agent still starts its main session", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  const onSetUpProjectHome = cy.stub().as("onSetUpProjectHome");
  aDrawer({ onEnsure, onSetUpProjectHome });

  // When
  byTestId(sessionsDrawerProjectHome(ALPHA.projectId)).click();

  // Then
  cy.get("@onEnsure").should("have.been.calledOnceWith", ALPHA.projectId);
  cy.get("@onSetUpProjectHome").should("not.have.been.called");
});

it("opening Projects for a project focuses that project's home agent picker", () => {
  // Given the address the drawer sends the operator to
  window.location.hash = `#/projects?project=${BETA.projectId}`;

  // When
  theProjectsPage();

  // Then
  projectsScreenPage.homeAgentSelect(BETA.projectId).should("have.focus");
});
