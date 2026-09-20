/**
 * Acceptance tests: the new-session form can ask for the **sandboxed codebase** placement — the
 * session's own checkout inside a `--workspace-tools` jail, with the agent running beside it,
 * unconfined, and its native filesystem and shell tools withdrawn.
 *
 * Three toggles now name a placement, and a session has exactly one, so choosing any of them
 * clears the other two **in the form**. A request that disagrees with the screen is how a chosen
 * option went missing without an error.
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
import type { DaemonHost } from "../../src/lib/participantRole";
import { daemonRpcIdentity } from "../../src/lib/participantRole";
import { AuthProvider } from "../../src/hooks/authProvider";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aJoinedCommonRoom } from "../support/rpc/withSelectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";
import { anAvailableAgent } from "../support/rpc/sessionAgentRosterBackend";

const THIS_HOST = "workstation";
/** A specialized agent this deployment offers, by the qualified id the form submits. */
const FASTCONTEXT = "fastcontext@workstation";
const PROJECT = "proj-1";

/**
 * A host whose workspace jail confines the filesystem too — a macOS Seatbelt daemon. The placement
 * is offered plainly here, with nothing to caveat.
 */
const A_HOST_WITH_A_FULLY_CONFINING_JAIL: DaemonHost = {
  instanceId: THIS_HOST,
  label: "workstation (this daemon)",
  sandboxedCodebase: { confinesFilesystem: true },
};

/**
 * A host that serves the placement with a jail sharing the filesystem root — today's Linux cgroups
 * jail, whose minimal read-only root is unbuilt
 * (docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md).
 */
const A_HOST_WHOSE_JAIL_SHARES_THE_FILESYSTEM_ROOT: DaemonHost = {
  instanceId: THIS_HOST,
  label: "workstation (this daemon)",
  sandboxedCodebase: { confinesFilesystem: false },
};

/**
 * A daemon old enough not to advertise the capability at all. Absent is not false-with-a-reason:
 * it is a host that would answer an unrecognised field by starting an ordinary session.
 */
const A_HOST_THAT_DOES_NOT_ADVERTISE_THE_PLACEMENT: DaemonHost = {
  instanceId: THIS_HOST,
  label: "workstation (this daemon)",
};

/** The same backend, with one specialized agent on offer. */
function aCreateSessionBackendOfferingAnAgent(): InMemoryRpcBackend {
  return aCreateSessionBackend().onUnary(CatalogService.method.listSubagents, () => ({
    subagents: [anAvailableAgent("fastcontext", THIS_HOST)],
  }));
}

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

function mountCreatePane(backend: InMemoryRpcBackend, host: DaemonHost) {
  const client = createClient(SessionService, backend.transport());
  const projectClient = createClient(ProjectService, backend.transport());
  const catalogClient = createClient(CatalogService, backend.transport());
  const sessionFilesClient = createClient(SessionFilesService, backend.transport());
  const worktreeClient = createClient(WorktreeService, backend.transport());
  mountWithRpc(
    <AuthProvider>
      <SelectedDaemonProvider
        room={aJoinedCommonRoom([daemonRpcIdentity(host.instanceId)])}
        daemons={[host]}
        servingInstanceId={host.instanceId}
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

/** The one `StartSession` the form sent. More than one means the test drove the form twice. */
function theStartSessionRequest(backend: InMemoryRpcBackend) {
  const calls = backend.callsTo(SessionService.method.startSession);
  expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
  return calls[0];
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

describe("Create session sandboxed codebase", () => {
  let rpcBackend: InMemoryRpcBackend;

  it("sends a sandboxed codebase placement on the start request", () => {
    // Given a claude-cli session on a host that serves the placement
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);

    // When the operator jails the codebase
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.submit();

    // Then the request names that placement and no other
    cy.wrap(rpcBackend).should(() => {
      const request = theStartSessionRequest(rpcBackend);
      expect(request.sessionType).to.equal("claude-cli");
      expect(request.sandboxedCodebase).to.equal(true);
      expect(request.sandbox).to.equal(false);
      expect(request.managedCodebase).to.equal(false);
      expect(request.codebaseDaemonInstanceId).to.equal("");
    });
  });

  it("clears the managed codebase placement in the form when the codebase is jailed", () => {
    // Given the operator has chosen the managed placement
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.enableManagedCodebase();
    createSessionPage.managedCodebaseToggle().should("be.checked");

    // When they jail the codebase instead
    createSessionPage.enableSandboxedCodebase();

    // Then the managed placement is cleared on screen, not silently at submit
    createSessionPage.managedCodebaseToggle().should("not.be.checked");
    createSessionPage.sandboxedCodebaseToggle().should("be.checked");
  });

  it("clears the agent sandbox placement in the form when the codebase is jailed", () => {
    // Given the operator has chosen to jail the agent
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.enableSandbox();
    createSessionPage.sandboxToggle().should("be.checked");

    // When they jail the codebase instead
    createSessionPage.enableSandboxedCodebase();

    // Then the agent sandbox is cleared — the two name opposite placements
    createSessionPage.sandboxToggle().should("not.be.checked");
    createSessionPage.sandboxedCodebaseToggle().should("be.checked");
  });

  it("clears the sandboxed codebase placement in the form when managed codebase is chosen", () => {
    // Given the operator has jailed the codebase
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.sandboxedCodebaseToggle().should("be.checked");

    // When they choose the managed placement instead
    createSessionPage.enableManagedCodebase();

    // Then the jailed-codebase placement is cleared
    createSessionPage.sandboxedCodebaseToggle().should("not.be.checked");
    createSessionPage.managedCodebaseToggle().should("be.checked");
  });

  it("clears the sandboxed codebase placement in the form when the agent sandbox is chosen", () => {
    // Given the operator has jailed the codebase
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.sandboxedCodebaseToggle().should("be.checked");

    // When they choose to jail the agent instead
    createSessionPage.enableSandbox();

    // Then the jailed-codebase placement is cleared
    createSessionPage.sandboxedCodebaseToggle().should("not.be.checked");
    createSessionPage.sandboxToggle().should("be.checked");
  });

  it("stops offering to skip permissions once the codebase is jailed", () => {
    // Given a claude-cli session, which offers the bypass while no placement confines the agent
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.dangerouslySkipPermissionsToggle().should("exist");

    // When the operator jails the codebase
    createSessionPage.enableSandboxedCodebase();

    // Then it is withdrawn. The agent runs unjailed beside its checkout and its entire "no route
    // to the host filesystem" guarantee rests on the deny list its argv withdraws; whether that
    // list survives --dangerously-skip-permissions is not something this repo pins, so the
    // combination is not offered rather than assumed safe — and the daemon refuses it by name.
    createSessionPage.dangerouslySkipPermissionsToggle().should("not.exist");
  });

  it("sends no permission bypass for a sandboxed codebase session", () => {
    // Given the bypass switched on before any placement was chosen
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);
    createSessionPage.dangerouslySkipPermissionsToggle().check();

    // When the codebase is jailed and the session created
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.submit();

    // Then the stale choice does not ride along into a placement the daemon would refuse it on
    cy.wrap(rpcBackend).should(() => {
      const request = theStartSessionRequest(rpcBackend);
      expect(request.sandboxedCodebase).to.equal(true);
      expect(request.dangerouslySkipPermissions).to.equal(false);
    });
  });

  it("offers no sandboxed codebase placement for a session type that cannot withdraw its tools", () => {
    // Given a host that serves the placement
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);

    // When the operator picks cursor-cli, whose tool surface has no --disallowedTools equivalent
    createSessionPage.switchToCursorCliSession();

    // Then the placement is not offered — confinement there would confine nothing
    createSessionPage.expectNoSandboxedCodebaseToggle();
  });

  it("disables the sandboxed codebase placement and states why on a host that cannot serve it", () => {
    // Given a daemon that does not advertise the placement
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_THAT_DOES_NOT_ADVERTISE_THE_PLACEMENT);

    // When the operator opens a claude-cli session on it
    createSessionPage.switchToClaudeCliSession();

    // Then the control is visible and disabled, and the reason names the host
    createSessionPage.sandboxedCodebaseToggle().should("be.visible").and("be.disabled");
    createSessionPage
      .sandboxedCodebaseUnavailableReason()
      .should("be.visible")
      .and("contain.text", "workstation");
  });

  it("never submits a sandboxed codebase placement a disabled control could not have chosen", () => {
    // Given a daemon that does not advertise the placement
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_THAT_DOES_NOT_ADVERTISE_THE_PLACEMENT);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);

    // When the operator submits without touching the disabled control
    createSessionPage.submit();

    // Then the request carries no placement the host could not serve
    cy.wrap(rpcBackend).should(() => {
      expect(theStartSessionRequest(rpcBackend).sandboxedCodebase).to.equal(false);
    });
  });

  it("states what the jail does not confine on a host whose jail shares the filesystem root", () => {
    // Given a host serving the placement with a jail that shares the filesystem root
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WHOSE_JAIL_SHARES_THE_FILESYSTEM_ROOT);

    // When the operator opens a claude-cli session on it
    createSessionPage.switchToClaudeCliSession();

    // Then the placement is offered, and says what it does not confine
    createSessionPage.sandboxedCodebaseToggle().should("be.enabled");
    createSessionPage
      .sandboxedCodebaseCaveat()
      .should("be.visible")
      .and("contain.text", "outside the checkout");
  });

  it("states no caveat on a host whose jail confines the filesystem", () => {
    // Given a host whose jail confines the filesystem too
    rpcBackend = aCreateSessionBackend();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);

    // When the operator opens a claude-cli session on it
    createSessionPage.switchToClaudeCliSession();

    // Then there is nothing to caveat
    createSessionPage.sandboxedCodebaseToggle().should("be.enabled");
    createSessionPage.sandboxedCodebaseCaveat().should("not.exist");
  });

  it("offers the specialized-agent picker when the codebase is jailed", () => {
    // Given a host that serves the placement and offers an agent
    rpcBackend = aCreateSessionBackendOfferingAnAgent();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);

    // When the operator jails the codebase
    createSessionPage.enableSandboxedCodebase();

    // Then the agent is still on offer. Delegation is orthogonal to confinement, and this is the
    // placement where it is safest — the checkout an agent could damage is the jailed one.
    createSessionPage.specializedAgentOption(FASTCONTEXT).should("be.visible");
  });

  it("sends the chosen specialized agents for a jailed-codebase session", () => {
    // Given an agent chosen on a jailed-codebase session
    rpcBackend = aCreateSessionBackendOfferingAnAgent();
    mountCreatePane(rpcBackend, A_HOST_WITH_A_FULLY_CONFINING_JAIL);
    createSessionPage.switchToClaudeCliSession();
    createSessionPage.selectProject(PROJECT);
    createSessionPage.enableSandboxedCodebase();
    createSessionPage.selectSpecializedAgent(FASTCONTEXT);

    // When the session is created
    createSessionPage.submit();

    // Then the selection rides along beside the placement rather than being dropped with the
    // control it used to live under
    cy.wrap(rpcBackend).should(() => {
      const request = theStartSessionRequest(rpcBackend);
      expect(request.sandboxedCodebase).to.equal(true);
      expect(request.specializedAgents).to.deep.equal([FASTCONTEXT]);
    });
  });
});
