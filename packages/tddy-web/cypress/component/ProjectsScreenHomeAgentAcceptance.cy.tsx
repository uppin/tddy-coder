/**
 * Acceptance: each project card on the Projects screen shows the project's **home agent** — the
 * agent its project-home session runs — and changes it through `SetProjectHomeAgent`. A refused
 * change is shown on that card.
 *
 * Spec: docs/ft/web/projects-screen-multi-host.md (§ Home agent) (`#project-home` 1/7).
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import { AgentInfoSchema, CatalogService } from "../../src/gen/catalog_pb";
import { ProjectEntrySchema, ProjectService, type ProjectEntry } from "../../src/gen/project_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { mountWithRpc } from "../support/rpc/inMemory";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

const LOCAL_HOST = "workstation-1";
const DAEMON_HOSTS: DaemonHost[] = [{ instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" }];

const CLAUDE = create(AgentInfoSchema, { id: "claude", label: "Claude" });
const CLAUDE_ON_META = create(AgentInfoSchema, { id: "claude-on-meta", label: "Claude Code · Meta" });

function aProject(overrides: Partial<ProjectEntry>): ProjectEntry {
  return create(ProjectEntrySchema, {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: LOCAL_HOST,
    mainBranchRef: "",
    defaultRemote: "",
    accounts: [],
    localGh: false,
    homeAgent: "",
    ...overrides,
  });
}

/** A daemon listing `projects` and the two catalog agents, storing a home agent it is sent. */
function aDaemonWithAgents(projects: ProjectEntry[]): InMemoryRpcBackend {
  const state = [...projects];
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects: state }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [CLAUDE, CLAUDE_ON_META] }))
    .onUnary(ProjectService.method.setProjectHomeAgent, (req) => {
      const project = state.find((p) => p.projectId === req.projectId)!;
      project.homeAgent = req.homeAgent;
      return { project };
    });
}

/** A daemon that refuses every home agent change with `reason`. */
function aDaemonRefusingHomeAgents(projects: ProjectEntry[], reason: string): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ProjectService.method.listProjects, () => ({ projects }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({ branches: [], defaultRemote: "origin" }))
    .onUnary(CatalogService.method.listAgents, () => ({ agents: [CLAUDE, CLAUDE_ON_META] }))
    .onUnary(ProjectService.method.setProjectHomeAgent, () => {
      throw new ConnectError(reason, Code.InvalidArgument);
    });
}

function theProjectsPage() {
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
        <ProjectsAppPage onNavigate={cy.stub()} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("the projects screen shows and changes a project's home agent", () => {
  // Given a project whose home agent is Claude, on a daemon listing two agents
  const backend = aDaemonWithAgents([aProject({ projectId: "proj-alpha", homeAgent: CLAUDE.id })]);
  mountWithRpc(theProjectsPage(), backend);
  projectsScreenPage.homeAgentValue("proj-alpha").should("equal", CLAUDE.id);
  projectsScreenPage
    .homeAgentOptionLabels("proj-alpha")
    .should("deep.equal", ["Not set", CLAUDE.label, CLAUDE_ON_META.label]);

  // When the operator picks the harness profile
  projectsScreenPage.setHomeAgent("proj-alpha", CLAUDE_ON_META.id);

  // Then it is stored for that project on its host, and shown
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ProjectService.method.setProjectHomeAgent);
    expect(calls).to.have.length(1);
    expect(calls[0]).to.include({
      projectId: "proj-alpha",
      homeAgent: CLAUDE_ON_META.id,
      daemonInstanceId: LOCAL_HOST,
    });
  });
  projectsScreenPage.homeAgentValue("proj-alpha").should("equal", CLAUDE_ON_META.id);
});

it("a refused home agent change shows the reason on the row", () => {
  // Given a daemon that refuses the change
  const reason = "agent claude-on-meta is not listed by this daemon";
  const backend = aDaemonRefusingHomeAgents([aProject({ projectId: "proj-alpha" })], reason);
  mountWithRpc(theProjectsPage(), backend);

  // When the operator picks an agent
  projectsScreenPage.setHomeAgent("proj-alpha", CLAUDE_ON_META.id);

  // Then the card says why, and still shows the stored value
  projectsScreenPage.homeAgentError("proj-alpha").should("contain", reason);
  projectsScreenPage.homeAgentValue("proj-alpha").should("equal", "");
});
