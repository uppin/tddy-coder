/**
 * Acceptance: creating a `pr-stack` orchestrator that comes up already stacked on a session the
 * operator has open.
 *
 * The picker hangs off the **recipe**, not the branch mode. An orchestrator has no git branch of its
 * own, so there is no branch mode for the control to qualify — and offering it beside any other
 * recipe would offer a stack to a session that has none.
 *
 * The default option is empty and sends nothing, so a `pr-stack` session created the way every
 * existing caller creates one issues byte-for-byte the request it always has.
 *
 * Feature: docs/ft/coder/pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13
 */

import React from "react";
import { Code, ConnectError, createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { CreateSessionPane } from "../../src/components/sessions/CreateSessionPane";
import { ConnectionService } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { createSessionPage } from "../support/pages/createSessionPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const PROJECT_ID = "proj-stack-base";

/** The host the form creates on — the fixture daemon `withSelectedDaemon` selects. */
const HOST_ID = "local";

/** A session working on a branch — the kind that can seed a stack. */
const SESSION_ON_BRANCH = "session-auth-store";
const BRANCH_ON_IT = "feat/auth-store";

/** A session that has not created its branch yet — it owns no ref to base anything onto. */
const SESSION_WITHOUT_BRANCH = "session-unstarted";

/**
 * A session in this project, on this host, belonging to no stack. `overrides` moves it out of exactly
 * one of those, which is how each scoping scenario describes the session it expects not to be offered.
 */
function aSession(
  sessionId: string,
  branch: string,
  overrides: { projectId?: string; daemonInstanceId?: string; orchestratorSessionId?: string } = {},
) {
  return {
    sessionId,
    projectId: PROJECT_ID,
    recipe: "tdd",
    orchestratorSessionId: "",
    branch,
    daemonInstanceId: HOST_ID,
    ...overrides,
  };
}

/** The daemon's answer to a StartSession that spawned the orchestrator. */
function aStartedOrchestrator() {
  return {
    sessionId: "orchestrator-stack-base-1",
    livekitRoom: "room-stack-base-1",
    livekitUrl: "ws://127.0.0.1:7880",
    livekitServerIdentity: "daemon",
  };
}

/**
 * A StartSession that refuses the chosen base, the way the daemon refuses it pre-spawn.
 *
 * A `ConnectError` specifically: connect's unary handler replaces any other thrown value with a flat
 * `"internal error"` before it reaches the wire, so a plain `Error` would test the form against a
 * message the real daemon never sends. `FailedPrecondition` is the code
 * `validate_stack_seed_base_session` returns for a branchless base session.
 */
function refusingTheBase(reason: string): () => never {
  return () => {
    throw new ConnectError(reason, Code.FailedPrecondition);
  };
}

/**
 * A backend that stubs the model catalog (so Create is enabled), lists the sessions the picker
 * draws from, and captures StartSession.
 *
 * `startSession` is the one handler scenarios vary — a refusal is the same backend with a different
 * answer to the same call, not a second backend.
 */
function aCreateSessionBackend(
  options: {
    /** The sessions `ListSessions` answers with — what the picker offers. */
    sessions?: ReturnType<typeof aSession>[];
    /** How the daemon answers the submitted StartSession. Defaults to a successful spawn. */
    startSession?: () => ReturnType<typeof aStartedOrchestrator>;
  } = {},
) {
  const sessions = options.sessions ?? [aSession(SESSION_ON_BRANCH, BRANCH_ON_IT)];
  return anInMemoryRpcBackend()
    .onUnary(ConnectionService.method.listProjects, () => ({
      projects: [
        {
          projectId: PROJECT_ID,
          name: "stack-base-project",
          gitUrl: "https://example.com/stack-base.git",
          mainRepoPath: "/home/dev/stack-base-project",
          mainBranchRef: "origin/master",
          daemonInstanceId: "local",
        },
      ],
    }))
    .onUnary(ConnectionService.method.listAgents, () => ({
      agents: [{ id: "claude", name: "Claude" }],
    }))
    .onUnary(ConnectionService.method.listTools, () => ({
      tools: [{ path: "/usr/bin/tddy-coder", version: "0.1.0" }],
    }))
    .onUnary(ConnectionService.method.listSessions, () => ({ sessions }))
    .onUnary(ConnectionService.method.listAgentModels, () => ({
      models: [{ id: "claude-opus-4-8", label: "Claude Opus 4.8" }],
      defaultModel: "claude-opus-4-8",
    }))
    .onUnary(ConnectionService.method.startSession, options.startSession ?? aStartedOrchestrator);
}

function mountPane(backend: ReturnType<typeof aCreateSessionBackend>) {
  const client = createClient(ConnectionService, backend.transport());
  cy.mount(
    withSelectedDaemon(
      <CreateSessionPane
        client={client}
        sessionToken="fake-token"
        onCancel={cy.stub()}
        onCreated={cy.stub()}
        initialValues={{ sessionType: "tool", projectId: PROJECT_ID, recipe: "pr-stack" }}
      />,
    ),
  );
}

function startSessionCalls(backend: ReturnType<typeof aCreateSessionBackend>) {
  return backend.callsTo(ConnectionService.method.startSession);
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
// D1-D3: the picker's gate is the recipe
// ---------------------------------------------------------------------------

it("offers the stack-base picker when a tool session's recipe is pr-stack", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPane(backend);

  // Then
  createSessionPage.expectPrStackBaseSessionPickerOffered();
});

it("hides the stack-base picker for a recipe that owns no stack", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPane(backend);
  createSessionPage.expectPrStackBaseSessionPickerOffered();

  // When the operator switches to a recipe with no stack to seed
  createSessionPage.selectRecipe("tdd");

  // Then
  createSessionPage.expectNoPrStackBaseSessionPicker();
});

it("hides the stack-base picker for a claude-cli session", () => {
  // Given a tool session offering the picker
  const backend = aCreateSessionBackend();
  mountPane(backend);
  createSessionPage.expectPrStackBaseSessionPickerOffered();

  // When the operator switches to a Claude CLI session
  createSessionPage.switchToClaudeCliSession();

  // Then the picker is gone — the orchestrator is a tool session, and a claude-cli form holds no
  // agent + tool-path + model triple valid for spawning one
  createSessionPage.expectNoPrStackBaseSessionPicker();
});

// ---------------------------------------------------------------------------
// D4, D5: what the picker offers
// ---------------------------------------------------------------------------

it("lists only sessions that own a branch as stack-base options", () => {
  // Given one session on a branch and one that never created its own
  const backend = aCreateSessionBackend({
    sessions: [aSession(SESSION_ON_BRANCH, BRANCH_ON_IT), aSession(SESSION_WITHOUT_BRANCH, "")],
  });

  // When
  mountPane(backend);

  // Then only the branch-owning session is offered, behind the empty default — a branchless base
  // would fail the spawn gate for every descendant
  createSessionPage.expectPrStackBaseSessionOptionValues(["", SESSION_ON_BRANCH]);
});

/**
 * The next three scenarios are one rule read three ways: a base session must be one this stack can
 * actually act on.
 *
 * A descendant node's worktree is created off `origin/<base branch>` **in the orchestrator's project on
 * the orchestrator's host**, so a branch from another repository or another daemon's checkout is not a
 * ref this stack can resolve; and a session another orchestrator already tracks would leave two
 * orchestrators holding repoint and pull authority over one branch. The daemon refuses all three before
 * it spawns — offering them here would only let the operator pick a refusal, and for a branch from
 * another repository it used to be worse than that: nothing refused it, and the failure landed later as
 * a git error on the first descendant, on an orchestrator that already looked seeded.
 */
it("offers no session from another project as a stack base", () => {
  // Given a branch that lives in a different repository
  const backend = aCreateSessionBackend({
    sessions: [
      aSession(SESSION_ON_BRANCH, BRANCH_ON_IT),
      aSession("session-other-repo", "feat/billing-api", { projectId: "proj-elsewhere" }),
    ],
  });

  // When
  mountPane(backend);

  // Then only the branch in this project is offered
  createSessionPage.expectPrStackBaseSessionOptionValues(["", SESSION_ON_BRANCH]);
});

it("offers no session from another host as a stack base", () => {
  // Given a branch that exists in another daemon's checkout
  const backend = aCreateSessionBackend({
    sessions: [
      aSession(SESSION_ON_BRANCH, BRANCH_ON_IT),
      aSession("session-other-host", "feat/remote-work", { daemonInstanceId: "host-b" }),
    ],
  });

  // When
  mountPane(backend);

  // Then only the branch on the host this orchestrator will run on is offered
  createSessionPage.expectPrStackBaseSessionOptionValues(["", SESSION_ON_BRANCH]);
});

it("offers no session that another orchestrator's stack already owns", () => {
  // Given a session that is already a node of another stack
  const backend = aCreateSessionBackend({
    sessions: [
      aSession(SESSION_ON_BRANCH, BRANCH_ON_IT),
      aSession("session-already-stacked", "feat/already-stacked", {
        orchestratorSessionId: "orchestrator-elsewhere",
      }),
    ],
  });

  // When
  mountPane(backend);

  // Then only the unowned branch is offered
  createSessionPage.expectPrStackBaseSessionOptionValues(["", SESSION_ON_BRANCH]);
});

it("labels each stack-base option with the branch it would seed", () => {
  // Given
  const backend = aCreateSessionBackend();

  // When
  mountPane(backend);

  // Then the operator picks by the branch, which is what the node is bound to — behind a default that
  // names what leaving it alone does
  createSessionPage.expectPrStackBaseSessionOptionLabels([
    "None (agent plans the stack)",
    `${SESSION_ON_BRANCH} — ${BRANCH_ON_IT}`,
  ]);
});

// ---------------------------------------------------------------------------
// D6-D8: what gets sent
// ---------------------------------------------------------------------------

it("sends the chosen session id as the stack base", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPane(backend);

  // When
  createSessionPage.selectPrStackBaseSession(SESSION_ON_BRANCH);
  createSessionPage.submit();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = startSessionCalls(b);
    expect(calls).to.have.length(1);
    expect(calls[0].prStackBaseSessionId).to.equal(SESSION_ON_BRANCH);
    expect(calls[0].recipe).to.equal("pr-stack");
  });
});

it("sends no stack base when the picker is left on its default", () => {
  // Given a form offering the picker, which the operator does not touch
  const backend = aCreateSessionBackend();
  mountPane(backend);
  createSessionPage.expectPrStackBaseSessionPickerOffered();

  // When the operator creates an orchestrator the way every existing caller does
  createSessionPage.submit();

  // Then the request is the one that has always been sent, and the agent plans the stack
  cy.wrap(backend).should((b) => {
    const calls = startSessionCalls(b);
    expect(calls).to.have.length(1);
    expect(calls[0].prStackBaseSessionId).to.equal("");
  });
});

it("sends no stack base when the chosen base is abandoned by switching recipes", () => {
  // Given a base chosen while the recipe still owned a stack
  const backend = aCreateSessionBackend();
  mountPane(backend);
  createSessionPage.expectPrStackBaseSessionPickerOffered();
  createSessionPage.selectPrStackBaseSession(SESSION_ON_BRANCH);

  // When the operator switches to a recipe that has no stack to seed, and creates the session
  createSessionPage.selectRecipe("tdd");
  createSessionPage.submit();

  // Then the abandoned choice is not submitted — the daemon *refuses* a base session named beside
  // another recipe, so leaking it would fail a creation the operator never asked to seed
  cy.wrap(backend).should((b) => {
    const calls = startSessionCalls(b);
    expect(calls).to.have.length(1);
    expect(calls[0].recipe).to.equal("tdd");
    expect(calls[0].prStackBaseSessionId).to.equal("");
  });
});

it("spawns the orchestrator alone, not a session for the stack it was seeded with", () => {
  // Given
  const backend = aCreateSessionBackend();
  mountPane(backend);

  // When
  createSessionPage.selectPrStackBaseSession(SESSION_ON_BRANCH);
  createSessionPage.submit();

  // Then the single spawn is the orchestrator's own — a session spawned for a stack node would name
  // that orchestrator as its stack parent. The work sessions come later, from the Planned-PRs panel,
  // so there is no second spawn here to half-fail
  cy.wrap(backend).should((b) => {
    const calls = startSessionCalls(b);
    expect(calls).to.have.length(1);
    expect(calls[0].stackParent).to.equal("");
  });
});

// ---------------------------------------------------------------------------
// D9: the refusal the operator sees
// ---------------------------------------------------------------------------

it("shows the daemon's refusal when the chosen base session cannot seed a stack", () => {
  // Given a daemon that refuses the base before spawning anything
  const backend = aCreateSessionBackend({
    startSession: refusingTheBase("stack base session 'session-unstarted' owns no branch"),
  });
  mountPane(backend);

  // When
  createSessionPage.selectPrStackBaseSession(SESSION_ON_BRANCH);
  createSessionPage.submit();

  // Then the reason is on the form, not swallowed behind a navigation to a session that was
  // never created
  createSessionPage.error().should("contain.text", "owns no branch");
});
