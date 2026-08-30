/**
 * Acceptance tests: the new-session form lets an operator place a session's *codebase* on a
 * different daemon than the one running the agent. The agent then reaches the worktree only through
 * `mcp__tddy-tools__*` over LiveKit — it has no repository on its own host.
 *
 * The chosen daemon rides on `StartSession` as `codebaseDaemonInstanceId`. It is only meaningful
 * alongside `managedCodebase`, and only for claude-cli sessions, so the control is gated on both and
 * its value must never leak into a request that the daemon would refuse.
 *
 * Hosts come from the common LiveKit room via a `SelectedDaemonProvider` fixture; RPCs run against
 * the in-memory ConnectRPC backend so the tests assert on the typed request actually sent.
 *
 * PRD: docs/ft/daemon/remote-managed-worktree.md.
 */

import React from "react";
import { Room } from "livekit-client";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import type { DaemonHost } from "../../src/lib/participantRole";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** The daemon running the agent. */
const AGENT_HOST = "laptop-a";
/** The daemon holding the checkout — the one an operator would pick as the codebase host. */
const CODEBASE_HOST = "workstation-b";

const DAEMON_HOSTS: DaemonHost[] = [
  { instanceId: AGENT_HOST, label: "laptop-a (this daemon)" },
  { instanceId: CODEBASE_HOST, label: "workstation-b" },
];

/** "Same as host" — the co-located default, sent as an empty `codebaseDaemonInstanceId`. */
const SAME_AS_HOST = "";

/** The specialized agent a host offers, under the qualified id the picker submits. */
const FASTCONTEXT = `fastcontext@${AGENT_HOST}`;

/** One agent as `ListSubagents` returns it. */
interface OfferedAgent {
  name: string;
  label: string;
  model: string;
  daemonInstanceId: string;
  agentId: string;
}

/**
 * The backend with one specialized agent on offer. Separate from the default because most of these
 * tests are about a placement, not a roster — but the ones that are need something to select.
 */
function aCreateSessionBackendOfferingAnAgent(): InMemoryRpcBackend {
  return aCreateSessionBackend([
    {
      name: "fastcontext",
      label: "FastContext",
      model: "microsoft/FastContext-1.0-4B-RL",
      daemonInstanceId: AGENT_HOST,
      agentId: FASTCONTEXT,
    },
  ]);
}

function aCreateSessionBackend(offeredAgents: OfferedAgent[] = []): InMemoryRpcBackend {
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions: [] }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [{ projectId: "proj-1", name: "Test Project", mainRepoPath: "/repo" }],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", label: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", label: "tddy-coder" }],
    }))
    .onUnary(ConnectionService.method.listSubagents, () => ({ subagents: offeredAgents }))
    .onUnary(ConnectionService.method.listProjectBranches, () => ({
      branches: ["origin/main"],
      defaultRemote: "origin",
    }))
    .onUnary(ConnectionService.method.startSession, () => ({ sessionId: "split-1" }));
}

function mountCreatePane(backend: InMemoryRpcBackend) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    <SelectedDaemonProvider room={new Room()} daemons={DAEMON_HOSTS} servingInstanceId={AGENT_HOST}>
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
      />
    </SelectedDaemonProvider>,
  );
}

/** The single StartSession the form sent. */
function theStartSessionRequest(backend: InMemoryRpcBackend) {
  const calls = backend.callsTo(ConnectionService.method.startSession);
  expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
  return calls[0];
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearAllSessionStorage();
});

// ---------------------------------------------------------------------------
// Tests — availability of the control
// ---------------------------------------------------------------------------

it("offers the codebase host only once managed codebase is enabled", () => {
  // Given a claude-cli session with the managed-codebase section closed
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // Then — an agent that still holds native filesystem tools has nothing to proxy, so the
  // placement choice is not offered
  createSessionPage.expectNoCodebaseHostSelector();

  // When the section is opened
  createSessionPage.enableManagedCodebase();

  // Then it becomes available
  createSessionPage.codebaseHostSelect().should("be.visible");
});

it("offers the codebase host for claude-cli sessions but not for cursor-cli", () => {
  // Given a cursor-cli session with managed codebase enabled
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToCursorCliSession();
  createSessionPage.enableManagedCodebase();

  // Then — cursor-agent cannot enforce a tool allowlist, so split placement is claude-cli only
  createSessionPage.expectNoCodebaseHostSelector();

  // When the same managed session becomes a claude-cli one
  createSessionPage.switchToClaudeCliSession();

  // Then the choice appears — the gate is the session type, not the managed-codebase flag
  createSessionPage.codebaseHostSelect().should("be.visible");
});

it("offers same-as-host plus every common-room daemon as a codebase host", () => {
  // Given
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // When
  createSessionPage.enableManagedCodebase();

  // Then
  createSessionPage
    .codebaseHostOptionValues()
    .should("deep.equal", [SAME_AS_HOST, AGENT_HOST, CODEBASE_HOST]);
});

it("defaults the codebase host to same as host", () => {
  // Given
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();

  // When
  createSessionPage.enableManagedCodebase();

  // Then — the default placement is co-located, matching every session created before this feature
  createSessionPage.codebaseHostSelect().should("have.value", SAME_AS_HOST);
  createSessionPage.codebaseHostOptionLabels().should("have.length.at.least", 1);
  createSessionPage
    .codebaseHostOptionLabels()
    .then((labels) => expect(labels[0]).to.equal("Same as host"));
});

// ---------------------------------------------------------------------------
// Tests — what reaches StartSession
// ---------------------------------------------------------------------------

it("sends the chosen codebase host on the start session request", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When — the worktree is placed on the workstation while the agent runs on the laptop
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.sessionType).to.equal("claude-cli");
    expect(request.managedCodebase).to.equal(true);
    expect(request.daemonInstanceId).to.equal(AGENT_HOST);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
  });
});

it("sends an empty codebase host when the codebase stays on the session host", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When — the operator leaves the placement alone
  createSessionPage.submit();

  // Then — a co-located request is indistinguishable from one sent before this feature existed
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});

it("drops a chosen codebase host when the session type changes to cursor-cli", () => {
  // Given a codebase host chosen on a claude-cli session
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // When — the operator switches to a session type that cannot be split
  createSessionPage.switchToCursorCliSession();
  createSessionPage.submit();

  // Then — the two managed-codebase blocks share state, so the stale value must not survive into a
  // request the daemon would refuse
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.sessionType).to.equal("cursor-cli");
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});

// ---------------------------------------------------------------------------
// Tests — a split session cannot carry a workflow recipe
// ---------------------------------------------------------------------------
//
// A recipe's tooling runs against a repository on the daemon hosting the agent, and a split
// session has none — the daemon refuses the combination outright. The form defaults `recipe` to
// "tdd" and sends it whenever managed codebase is on, so without this the *only* thing the codebase
// host selector could produce is a request the daemon rejects.

it("stops offering a workflow recipe once the codebase is placed on another host", () => {
  // Given a managed claude-cli session, which offers a recipe while the codebase is co-located
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.enableManagedCodebase();
  createSessionPage.recipeSelect().should("be.visible");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then the recipe is no longer offered, rather than offered and then silently refused
  createSessionPage.expectNoRecipeSelector();
});

it("sends no workflow recipe for a split session", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then — the form's default recipe must not ride along and turn a valid placement into a refusal
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
    expect(request.recipe).to.equal("");
  });
});

it("stops offering sandbox once the codebase is placed on another host", () => {
  // Given a managed claude-cli session, which offers the sandbox while the codebase is co-located
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.enableManagedCodebase();
  createSessionPage.sandboxToggle().should("exist");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then — a sandbox resolves a worktree on the daemon running the agent, which a split session
  // does not have, so the daemon refuses it. Offering it would be the same trap the recipe had.
  createSessionPage.sandboxToggle().should("not.exist");
});

it("keeps the semantic index on offer once the codebase is placed on another host", () => {
  // Given a managed claude-cli session with the index on offer while the codebase is co-located
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.enableManagedCodebase();
  createSessionPage.semanticIndexToggle().should("exist");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then it stays on offer — the index is built wherever the worktree is, and a split session has
  // one, on the codebase host. Withdrawing it here would hide a choice the daemon accepts.
  createSessionPage.semanticIndexToggle().should("exist");
});

it("sends the chosen semantic index for a split session, and no sandbox", () => {
  // Given both switched on while the codebase was still co-located
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.sandboxToggle().check();
  createSessionPage.semanticIndexToggle().check();

  // When the codebase is placed elsewhere and the session created
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then the index rides along to the host that can build it, while the sandbox — which the daemon
  // refuses on this placement — does not survive into the request
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
    expect(request.sandbox).to.equal(false);
    expect(request.semanticIndex).to.equal(true);
  });
});

// ---------------------------------------------------------------------------
// Tests — a split session is seeded with a specialized agent like any other
// ---------------------------------------------------------------------------
//
// An agent is placeable on any host, and the placement decides how it reads the codebase rather than
// whether it may be picked: an agent on the codebase host reads that worktree directly, one anywhere
// else reads a clone the session's worktree sync keeps current. So a split placement withdraws
// nothing from the picker — the operator picks agents, and the daemon works out the plumbing.

it("keeps the specialized-agent picker once the codebase is placed on another host", () => {
  // Given a managed claude-cli session with an agent on offer while the codebase is co-located
  mountCreatePane(aCreateSessionBackendOfferingAnAgent());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.specializedAgentOption(FASTCONTEXT).should("be.visible");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then the agent is still on offer — where the codebase sits is not a reason to withhold it
  createSessionPage.specializedAgentOption(FASTCONTEXT).should("be.visible");
});

it("sends the chosen specialized agents for a split session", () => {
  // Given an agent picked while the codebase was still co-located
  const backend = aCreateSessionBackendOfferingAnAgent();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectSpecializedAgent(FASTCONTEXT);

  // When the operator then places the codebase on another host and creates the session
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then the selection rides along: the daemon seeds the roster beside the codebase and provisions
  // whatever the agent's own placement needs, so dropping it here would silently start a different
  // session than the one that was asked for
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
    expect(request.specializedAgents).to.deep.equal([FASTCONTEXT]);
  });
});

it("stops offering to skip permissions once the codebase is placed on another host", () => {
  // Given a managed claude-cli session, which offers the toggle while the codebase is co-located
  mountCreatePane(aCreateSessionBackend());
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.enableManagedCodebase();
  createSessionPage.dangerouslySkipPermissionsToggle().should("exist");

  // When the codebase moves to another host
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // Then it is withdrawn. A split session runs unjailed on this host and its entire "no route to
  // the local filesystem" guarantee rests on the agent's deny list; whether that list survives
  // --dangerously-skip-permissions is not something this repo pins, so the combination is not
  // offered rather than assumed safe.
  createSessionPage.dangerouslySkipPermissionsToggle().should("not.exist");
});

it("sends no permission bypass for a split session", () => {
  // Given the bypass switched on while the codebase was still co-located
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.dangerouslySkipPermissionsToggle().check();

  // When the codebase is placed elsewhere and the session created
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.submit();

  // Then the stale choice does not ride along into the one session type that has no jail behind it
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal(CODEBASE_HOST);
    expect(request.dangerouslySkipPermissions).to.equal(false);
  });
});

it("treats naming the session's own host as co-located, keeping the recipe", () => {
  // Given a managed claude-cli session on the agent host
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();

  // When the operator names that same host as the codebase host — the explicit spelling of
  // "co-located", which the daemon classifies exactly that way
  createSessionPage.selectCodebaseHost(AGENT_HOST);

  // Then the form must agree with the daemon rather than treating it as a split: withdrawing the
  // recipe here would launch a co-located session without the workflow it was created for
  createSessionPage.recipeSelect().should("be.visible");
  createSessionPage.submit();
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.recipe).to.equal("tdd");
  });
});

it("restores the workflow recipe when the codebase comes back to the session host", () => {
  // Given a split placement, with the recipe withdrawn
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);
  createSessionPage.expectNoRecipeSelector();

  // When the operator puts the codebase back on the session's own host
  createSessionPage.selectCodebaseHost(SAME_AS_HOST);
  createSessionPage.submit();

  // Then the recipe returns — withdrawing it is a property of the split, not a one-way door
  createSessionPage.recipeSelect().should("be.visible");
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.codebaseDaemonInstanceId).to.equal("");
    expect(request.recipe).to.equal("tdd");
  });
});

it("drops a chosen codebase host when managed codebase is switched back off", () => {
  // Given a codebase host chosen inside an open managed-codebase section
  const backend = aCreateSessionBackend();
  mountCreatePane(backend);
  createSessionPage.switchToClaudeCliSession();
  createSessionPage.selectProject("proj-1");
  createSessionPage.enableManagedCodebase();
  createSessionPage.selectCodebaseHost(CODEBASE_HOST);

  // When — the operator closes the section again
  createSessionPage.disableManagedCodebase();
  createSessionPage.submit();

  // Then — matching how specializedAgents and semanticIndex are cleared with the section
  cy.wrap(null).should(() => {
    const request = theStartSessionRequest(backend);
    expect(request.managedCodebase).to.equal(false);
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });
});
