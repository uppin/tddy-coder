/**
 * Reproduction: clicking a project's pinned **main** entry when the project has no home agent does
 * nothing.
 *
 * The entry renders `aria-disabled` and returns early from its click, and its only explanation is a
 * small grey hint under it. An operator who clicks "main" sees no response at all — no request, no
 * navigation, no error — and reads the drawer as broken. Expected: the click takes the operator to
 * the Projects screen with that project's home-agent picker focused, so the missing setting is one
 * choice away. A project that has a home agent still starts its main session as before.
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { SessionEntrySchema, type SessionEntry } from "../../src/gen/session_pb";
import { SessionDrawer, type ProjectHomeInfo } from "../../src/components/sessions/SessionDrawer";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import { AgentInfoSchema, CatalogService } from "../../src/gen/catalog_pb";
import { ProjectEntrySchema, ProjectService, type ProjectEntry } from "../../src/gen/project_pb";
import { AccountsService } from "../../src/gen/accounts_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";
import { byTestId, sessionsDrawerProjectHome } from "../support/testIds";

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [{ instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" }];

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

function aProject(projectId: string, name: string): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId,
    name,
    gitUrl: `https://example.com/${name}.git`,
    mainRepoPath: `/home/dev/repos/${name}`,
    daemonInstanceId: LOCAL_HOST,
    homeAgent: "",
  });
}

function theProjectsPage() {
  const backend = anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [aProject(ALPHA.projectId, ALPHA.name), aProject(BETA.projectId, BETA.name)],
    }) as never)
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }) as never)
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [create(AgentInfoSchema, { id: "claude", label: "Claude" })],
    }) as never)
    .onUnary(AccountsService.method.listAccounts, () => ({ providers: [] }) as never);
  mountWithRpc(
    <AuthProvider>
      <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
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
