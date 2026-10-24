/**
 * Reproduction: clicking a project's pinned **main** entry when the project has no main does
 * nothing.
 *
 * The entry renders `aria-disabled` and returns early from its click, and its only explanation is a
 * small grey hint under it. An operator who clicks "main" sees no response at all — no request, no
 * navigation, no error — and reads the drawer as broken. Expected: the click asks to set main up
 * for that project (the Sessions screen opens its setup pane — see
 * `ProjectHomeSetupPaneAcceptance`). A project whose main is set up still starts it as before. Each
 * Projects card links to the same setup (**Set up main**), or to main itself (**Open main**), as
 * recorded on the card's primary (first-listed) host.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { SessionDrawer, type ProjectHomeInfo } from "../../src/components/sessions/SessionDrawer";
import { ProjectService, type ProjectEntry } from "../../src/gen/project_pb";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  PROJECTS_LOCAL_HOST,
  PROJECTS_REMOTE_HOST,
  aProject,
  aProjectsBackend,
  projectsAppPage,
} from "../support/rpc/projectsAppPageFixtures";
import { projectHomeSetupPage as setup } from "../support/pages/projectHomeSetupPage";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };
const ALPHA_MAIN_SESSION = "home-alpha";
const ALPHA_HOME: ProjectHomeInfo = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeSessionId: ALPHA_MAIN_SESSION };
const BETA_NO_MAIN: ProjectHomeInfo = { projectId: BETA.projectId, mainBranchRef: "origin/master", homeSessionId: null };

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
        projectHomes={[ALPHA_HOME, BETA_NO_MAIN]}
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

/** The Projects page over `ListProjects` answering `projects`. */
function theProjectsPageListing(projects: ProjectEntry[]) {
  const backend = aProjectsBackend((b) =>
    b.onUnary(ProjectService.method.listProjects, () => ({ projects })),
  );
  mountWithRpc(projectsAppPage(), backend);
}

/** The Projects page listing ALPHA, whose main is set up, and BETA, which has none. */
function theProjectsPage() {
  const projects = [
    { ...ALPHA, homeSessionId: ALPHA_MAIN_SESSION },
    { ...BETA, homeSessionId: "" },
  ].map(({ projectId, name, homeSessionId }) =>
    aProject({
      projectId,
      name,
      gitUrl: `https://example.com/${name}.git`,
      mainRepoPath: `/home/dev/repos/${name}`,
      homeSessionId,
    }),
  );
  theProjectsPageListing(projects);
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "";
});

it("clicking main on a project with no main asks to set one up for that project", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  const onSetUpProjectHome = cy.stub().as("onSetUpProjectHome");
  aDrawer({ onEnsure, onSetUpProjectHome });

  // When
  setup.clickMain(BETA.projectId);

  // Then the operator is sent to set main up, and nothing is started
  cy.get("@onSetUpProjectHome").should("have.been.calledOnceWith", BETA.projectId);
  cy.get("@onEnsure").should("not.have.been.called");
});

it("offers the main entry of a project with no main as an action, not as disabled", () => {
  // When
  aDrawer({ onEnsure: cy.stub().resolves(), onSetUpProjectHome: cy.stub() });

  // Then
  setup.mainEntry(BETA.projectId).should("not.have.attr", "aria-disabled", "true");
});

it("clicking main on a project whose main is set up still starts it", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  const onSetUpProjectHome = cy.stub().as("onSetUpProjectHome");
  aDrawer({ onEnsure, onSetUpProjectHome });

  // When
  setup.clickMain(ALPHA.projectId);

  // Then
  cy.get("@onEnsure").should("have.been.calledOnceWith", ALPHA.projectId);
  cy.get("@onSetUpProjectHome").should("not.have.been.called");
});

it("a project with no main links to Set up main from its Projects card", () => {
  // When
  theProjectsPage();

  // Then
  setup
    .projectCardLink(BETA.projectId)
    .should("have.text", "Set up main")
    .and("have.attr", "href", `#/sessions/project-home/${BETA.projectId}`);
});

it("a project with a main links to Open main from its Projects card", () => {
  // When
  theProjectsPage();

  // Then
  setup
    .projectCardLink(ALPHA.projectId)
    .should("have.text", "Open main")
    .and("have.attr", "href", `#/sessions/${ALPHA_MAIN_SESSION}`);
});

it("a project on two hosts links to its primary host's main from its Projects card", () => {
  // When ALPHA is listed first on a host with no main for it, then on a host whose main is set up
  theProjectsPageListing([
    aProject({ ...ALPHA, daemonInstanceId: PROJECTS_LOCAL_HOST, homeSessionId: "" }),
    aProject({ ...ALPHA, daemonInstanceId: PROJECTS_REMOTE_HOST, homeSessionId: ALPHA_MAIN_SESSION }),
  ]);

  // Then the card's link is the first-listed (primary) host's
  setup
    .projectCardLink(ALPHA.projectId)
    .should("have.text", "Set up main")
    .and("have.attr", "href", `#/sessions/project-home/${ALPHA.projectId}`);
});
