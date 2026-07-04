/**
 * Acceptance tests: the dedicated Projects screen (`/projects`) lists projects grouped by host and
 * lets an operator add an existing project to another host (reusing its project id).
 *
 * Host selection is driven by the daemon-role participants in the common LiveKit room, so the
 * host-dependent behaviors are exercised against the presentational `ProjectsScreen` with an
 * explicit `daemons` prop; the RPC-wiring behaviors (list/create) are exercised against the
 * `ProjectsAppPage` container via the in-memory backend.
 *
 * PRD: docs/ft/web/projects-screen-multi-host.md.
 */

import React from "react";
import { Room } from "livekit-client";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { ProjectsAppPage } from "../../src/components/projects/ProjectsAppPage";
import { ProjectsScreen } from "../../src/components/projects/ProjectsScreen";
import { ConnectionService, type ProjectEntry } from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { mountWithRpc } from "../support/rpc/inMemory";
import { mountWithRecordingLiveKitRpc } from "../support/rpc/recordingLiveKitRpc";
import { projectsScreenPage } from "../support/pages/projectsScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const LOCAL_HOST = "workstation-1";
const REMOTE_HOST = "server-2";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)" },
  { instanceId: REMOTE_HOST, label: "server-2 (this daemon)" },
];

function aProject(overrides: Partial<ProjectEntry>): ProjectEntry {
  return {
    projectId: "proj-alpha",
    name: "alpha",
    gitUrl: "https://example.com/alpha.git",
    mainRepoPath: "/home/dev/repos/alpha",
    daemonInstanceId: LOCAL_HOST,
    ...overrides,
  } as ProjectEntry;
}

/** In-memory backend pre-seeded with the RPCs `ProjectsAppPage` calls on startup. */
function aProjectsBackend(projects: ProjectEntry[]): InMemoryRpcBackend {
  const state = [...projects];
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listProjects, () => ({ projects: state }))
    .onUnary(ConnectionService.method.createProject, (req) => {
      const project = aProject({
        projectId: "proj-new",
        name: req.name,
        gitUrl: req.gitUrl,
        daemonInstanceId: LOCAL_HOST,
      });
      state.push(project);
      return { project };
    });
}

/**
 * `ProjectsAppPage` now reads its daemon list and RPC client from the shared
 * `SelectedDaemonProvider` context (see `DaemonSelectedRpcRoutingAcceptance.cy.tsx`) instead of
 * opening its own common-room connection — these container tests only care about the RPC-wiring
 * behavior, so a fixed single-host context is enough to get a non-null `useDaemonClient`.
 */
function mountProjectsAppPage(onNavigate: (path: string) => void) {
  return (
    <AuthProvider>
      <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={LOCAL_HOST}>
        <ProjectsAppPage onNavigate={onNavigate} />
      </SelectedDaemonProvider>
    </AuthProvider>
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Container behaviors (RPC wiring)
// ---------------------------------------------------------------------------

it("renders a project present on two hosts as one card with a row per host", () => {
  // Given
  const backend = aProjectsBackend([
    aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST, mainRepoPath: "/home/dev/alpha" }),
    aProject({ projectId: "proj-alpha", daemonInstanceId: REMOTE_HOST, mainRepoPath: "/srv/alpha" }),
  ]);

  // When
  mountWithRpc(mountProjectsAppPage(cy.stub()), backend);

  // Then
  projectsScreenPage.card("proj-alpha").should("exist");
  projectsScreenPage
    .hostRowDaemonIds("proj-alpha")
    .should("deep.equal", [LOCAL_HOST, REMOTE_HOST]);
});

it("creates a project from the screen and shows it after the list refreshes", () => {
  // Given
  const backend = aProjectsBackend([]);

  // When
  mountWithRpc(mountProjectsAppPage(cy.stub()), backend);
  projectsScreenPage.openCreateProjectForm();
  projectsScreenPage.fillAndSubmitCreateProjectForm({
    name: "beta",
    gitUrl: "https://example.com/beta.git",
  });

  // Then
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.createProject)).to.have.length(1);
  });
  projectsScreenPage.card("proj-new").should("exist");
});

// ---------------------------------------------------------------------------
// Host selection (driven by the daemon-role participant list)
// ---------------------------------------------------------------------------

it("adds a project to the selected host reusing the project's existing id", () => {
  // Given a project on the local host and both hosts available as daemons
  const onAddProjectToHost = cy.stub().as("addProjectToHost");
  cy.mount(
    <ProjectsScreen
      projects={[aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST })]}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub()}
      onAddProjectToHost={onAddProjectToHost}
    />,
  );

  // When
  projectsScreenPage.openAddToHost("proj-alpha");
  projectsScreenPage.addProjectToHost("proj-alpha", REMOTE_HOST);

  // Then
  cy.get("@addProjectToHost").should("have.been.calledWithMatch", {
    projectId: "proj-alpha",
    daemonInstanceId: REMOTE_HOST,
  });
});

it("offers only hosts that do not already host the project as add-to-host targets", () => {
  // Given the project already lives on the local host
  cy.mount(
    <ProjectsScreen
      projects={[aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST })]}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub()}
      onAddProjectToHost={cy.stub()}
    />,
  );

  // When
  projectsScreenPage.openAddToHost("proj-alpha");

  // Then — only the remote host (not the already-hosting local host) is selectable
  projectsScreenPage.addToHostOptionValues("proj-alpha").should("deep.equal", [REMOTE_HOST]);
});

// ---------------------------------------------------------------------------
// Add to host — routing directly to the chosen host, clone location, base location
// ---------------------------------------------------------------------------

it("sends the add-to-host RPC directly to the chosen host's daemon", () => {
  // Given a project that lives only on the local host, with a remote host also available. The
  // selected daemon (which serves the screen) is the local host.
  const backend = aProjectsBackend([
    aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST }),
  ]).onUnary(ConnectionService.method.addProjectToHost, () => ({
    project: aProject({ projectId: "proj-alpha", daemonInstanceId: REMOTE_HOST }),
  }));

  // When — the recording transport captures the RPC-server identity every LiveKit client is built for
  const { targets } = mountWithRecordingLiveKitRpc(mountProjectsAppPage(cy.stub()), backend);
  projectsScreenPage.openAddToHost("proj-alpha");
  projectsScreenPage.addProjectToHost("proj-alpha", REMOTE_HOST);

  // Then — the add-to-host RPC is addressed to the chosen host's own RPC identity
  // (daemon-server-2), not only the selected local daemon it double-hops through today.
  cy.wrap(null).should(() => {
    expect(targets).to.include(daemonRpcIdentity(REMOTE_HOST));
    const calls = backend.callsTo(ConnectionService.method.addProjectToHost);
    expect(calls).to.have.length(1);
    expect(calls[0].daemonInstanceId).to.equal(REMOTE_HOST);
  });
});

it("adds the project to the chosen host at the entered clone location", () => {
  // Given a project on the local host and a remote host available as a target
  const onAddProjectToHost = cy.stub().as("addProjectToHost");
  cy.mount(
    <ProjectsScreen
      projects={[aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST })]}
      daemons={DAEMON_HOSTS}
      onCreateProject={cy.stub()}
      onAddProjectToHost={onAddProjectToHost}
    />,
  );

  // When
  projectsScreenPage.openAddToHost("proj-alpha");
  projectsScreenPage.addProjectToHostWithLocation("proj-alpha", REMOTE_HOST, "work/alpha");

  // Then — the chosen clone location travels with the add-to-host request as userRelativePath
  cy.get("@addProjectToHost").should("have.been.calledWithMatch", {
    projectId: "proj-alpha",
    daemonInstanceId: REMOTE_HOST,
    userRelativePath: "work/alpha",
  });
});

it("shows the base clone location advertised by a hosting daemon", () => {
  // Given the local host advertises its base clone location
  const daemonsWithBase: DaemonHost[] = [
    { instanceId: LOCAL_HOST, label: "workstation-1 (this daemon)", reposBasePath: "repos" },
    { instanceId: REMOTE_HOST, label: "server-2 (this daemon)", reposBasePath: "srv/git" },
  ];
  cy.mount(
    <ProjectsScreen
      projects={[aProject({ projectId: "proj-alpha", daemonInstanceId: LOCAL_HOST })]}
      daemons={daemonsWithBase}
      onCreateProject={cy.stub()}
      onAddProjectToHost={cy.stub()}
    />,
  );

  // Then — the hosting daemon's base location is surfaced on its host row
  projectsScreenPage.hostBaseLocation(LOCAL_HOST).should("contain.text", "repos");
});
