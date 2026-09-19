/**
 * Acceptance tests: the **sandboxed codebase** placement is offered on a daemon with no common
 * room — the deployment it was designed for (PRD AC6: "the placement starts with no common room
 * configured").
 *
 * The capability reaches the web by two routes. The common room advertises it, and
 * `CreateSessionSandboxedCodebaseAcceptance` drives that one by injecting the host directly. But a
 * daemon with LiveKit unconfigured advertises nothing, and the page's only source of hosts is then
 * the daemon that served it — `/api/config`'s `sandboxed_codebase`, carried by
 * `useServingHostDirectorySource`. A spec that injects a `DaemonHost` bypasses both real sources
 * and would pass with that route missing entirely, which is exactly how the control came to render
 * disabled on every daemon that does not join a room.
 *
 * So every test here mounts with **no LiveKit source at all**: no `room`, no `daemons` override,
 * `livekitEnabled={false}`. What the form offers is what the serving daemon said about itself.
 *
 * PRD: docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
 * Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { SessionService } from "../../src/gen/session_pb";
import { ProjectService } from "../../src/gen/project_pb";
import { CatalogService } from "../../src/gen/catalog_pb";
import { SessionFilesService } from "../../src/gen/session_files_pb";
import { WorktreeService } from "../../src/gen/worktree_pb";
import { AuthProvider } from "../../src/hooks/authProvider";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { useServingHostDirectorySource } from "../../src/rpc/hostDirectory/servingSource";
import { daemonHostOf } from "../../src/rpc/hostDirectory/daemonHost";
import { sandboxedCodebaseUnavailability } from "../../src/components/sessions/codebasePlacement";
import { mountWithRpc } from "../support/rpc/inMemory";
import { createSessionPage } from "../support/pages/createSessionPage";
import { servingHostPage } from "../support/pages/servingHostPage";

const THIS_HOST = "workstation";
const PROJECT = "proj-1";

/** A macOS daemon: its Seatbelt jail denies paths outside the trees it holds. */
const A_JAIL_THAT_CONFINES_THE_FILESYSTEM = { confinesFilesystem: true };

/** Today's Linux daemon: its cgroups jail shares the host filesystem root. */
const A_JAIL_THAT_SHARES_THE_FILESYSTEM_ROOT = { confinesFilesystem: false };

function aCreateSessionBackend(): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(SessionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(CatalogService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ProjectService.method.listProjects, () => ({
      projects: [{ projectId: PROJECT, name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(CatalogService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(CatalogService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(CatalogService.method.listSubagents, () => ({ subagents: [] }))
    .onUnary(ProjectService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(SessionService.method.startSession, () => ({ sessionId: "sandboxed-codebase-1" }));
}

/**
 * The create-session form as a browser served by `servingSandboxedCodebase`'s daemon renders it,
 * with LiveKit switched off and no injected host list — so the serving source is the directory.
 */
function mountCreatePaneServedBy(
  backend: InMemoryRpcBackend,
  servingSandboxedCodebase?: { confinesFilesystem: boolean },
) {
  const client = createClient(SessionService, backend.transport());
  const projectClient = createClient(ProjectService, backend.transport());
  const catalogClient = createClient(CatalogService, backend.transport());
  const sessionFilesClient = createClient(SessionFilesService, backend.transport());
  const worktreeClient = createClient(WorktreeService, backend.transport());
  mountWithRpc(
    <AuthProvider>
      <SelectedDaemonProvider
        livekitEnabled={false}
        servingInstanceId={THIS_HOST}
        servingSandboxedCodebase={servingSandboxedCodebase}
      >
        <CreateSessionPane
          client={client}
          projectClient={projectClient}
          catalogClient={catalogClient}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          sessionToken="fake-token"
          onCancel={cy.stub()}
          onCreated={cy.stub()}
        />
      </SelectedDaemonProvider>
    </AuthProvider>,
    backend,
  );
}

/**
 * Renders what the serving source alone contributes: the host it names, the capability that host
 * carries, and the verdict the Start-Session form reaches from it.
 */
function ServingSourceProbe({
  sandboxedCodebase,
}: {
  sandboxedCodebase?: { confinesFilesystem: boolean };
}) {
  const source = useServingHostDirectorySource(THIS_HOST, sandboxedCodebase);
  const host = daemonHostOf(source.hosts[0]);
  return (
    <div>
      <div data-testid="serving-host-id">{host.instanceId}</div>
      <div data-testid="serving-host-jail">
        {host.sandboxedCodebase === undefined
          ? "unadvertised"
          : String(host.sandboxedCodebase.confinesFilesystem)}
      </div>
      <div data-testid="serving-host-placement-verdict">
        {sandboxedCodebaseUnavailability(host) ?? "available"}
      </div>
    </div>
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

describe("The serving daemon's own jail capability", () => {
  it("carries the capability its daemon described onto the host it contributes", () => {
    // Given a page served by a daemon whose jail confines the filesystem, and no common room

    // When the directory's serving source describes that daemon
    cy.mount(<ServingSourceProbe sandboxedCodebase={A_JAIL_THAT_CONFINES_THE_FILESYSTEM} />);

    // Then the host it names carries the capability, so the placement is available on it
    servingHostPage.expectHostId(THIS_HOST);
    servingHostPage.expectJail("true");
    servingHostPage.expectPlacementVerdict("available");
  });

  it("carries a jail that shares the filesystem root as available, not as withheld", () => {
    // Given a page served by a Linux daemon, whose jail confines process and network only

    // When the directory's serving source describes that daemon
    cy.mount(<ServingSourceProbe sandboxedCodebase={A_JAIL_THAT_SHARES_THE_FILESYSTEM_ROOT} />);

    // Then the placement is still available — what the jail does not confine is a caveat, not a
    // refusal
    servingHostPage.expectJail("false");
    servingHostPage.expectPlacementVerdict("available");
  });

  it("leaves the capability absent when its daemon described no jail", () => {
    // Given a page served by a daemon that predates the capability, or an OS with no jail

    // When the directory's serving source describes that daemon
    cy.mount(<ServingSourceProbe />);

    // Then absent stays absent, and the form may not assume a capability nobody described
    servingHostPage.expectJail("unadvertised");
    servingHostPage.expectPlacementVerdictContains("does not offer the sandboxed codebase");
  });
});

describe("Create session on a daemon with no common room", () => {
  it("offers the sandboxed codebase placement on a daemon that joins no common room", () => {
    // Given a claude-cli session on a daemon with LiveKit unconfigured, whose jail confines writes
    const backend = aCreateSessionBackend();
    mountCreatePaneServedBy(backend, A_JAIL_THAT_CONFINES_THE_FILESYSTEM);

    // When the operator opens the placement controls
    createSessionPage.switchToClaudeCliSession();

    // Then the control is offered rather than disabled — the deployment this placement exists for
    createSessionPage.sandboxedCodebaseToggle().should("not.be.disabled");
    createSessionPage.sandboxedCodebaseUnavailableReason().should("not.exist");
  });

  it("sends the sandboxed codebase placement chosen on a daemon that joins no common room", () => {
    // Given a claude-cli session on a daemon with LiveKit unconfigured
    const backend = aCreateSessionBackend();
    mountCreatePaneServedBy(backend, A_JAIL_THAT_CONFINES_THE_FILESYSTEM);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);

    // When the operator jails the codebase and starts the session
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.submit();

    // Then the placement reaches the wire, so the whole round trip works with no LiveKit at all
    cy.wrap(backend).should(() => {
      const calls = backend.callsTo(SessionService.method.startSession);
      expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
      expect(calls[0].sandboxedCodebase).to.equal(true);
    });
  });

  it("states what the jail does not confine on a daemon whose jail shares the filesystem root", () => {
    // Given a claude-cli session on a Linux daemon with LiveKit unconfigured
    const backend = aCreateSessionBackend();
    mountCreatePaneServedBy(backend, A_JAIL_THAT_SHARES_THE_FILESYSTEM_ROOT);

    // When the operator opens the placement controls
    createSessionPage.switchToClaudeCliSession();

    // Then the placement is offered with the caveat, never silently weaker than its name promises
    createSessionPage.sandboxedCodebaseToggle().should("not.be.disabled");
    createSessionPage.sandboxedCodebaseCaveat().should("be.visible");
  });

  it("disables the placement on a daemon that described no jail of its own", () => {
    // Given a claude-cli session on a daemon that advertises the capability nowhere
    const backend = aCreateSessionBackend();
    mountCreatePaneServedBy(backend, undefined);

    // When the operator opens the placement controls
    createSessionPage.switchToClaudeCliSession();

    // Then the control is disabled and says why, rather than silently downgrading the session
    createSessionPage.sandboxedCodebaseToggle().should("be.disabled");
    createSessionPage.sandboxedCodebaseUnavailableReason().should("be.visible");
  });
});
