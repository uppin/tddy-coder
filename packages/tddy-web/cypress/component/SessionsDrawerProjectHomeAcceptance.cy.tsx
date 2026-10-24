/**
 * Acceptance: each project group in the drawer starts with a pinned **main** entry for the
 * project-home session. Clicking it ensures that session (`EnsureProjectHomeSession`) and selects
 * it; the home session is not repeated among the group's rows; a project with no home agent starts
 * nothing and shows no hint (its click opens the setup pane); the **Other** group has no pinned entry; a project with a home agent and no
 * session yet still gets its group, so the pinned entry is how its first session starts.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry (`#project-home` 4/7).
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  SessionEntrySchema,
  SessionService,
  type SessionEntry,
} from "../../src/gen/session_pb";
import { SessionDrawer, type ProjectHomeInfo } from "../../src/components/sessions/SessionDrawer";
import { useEnsureProjectHome } from "../../src/components/sessions/useEnsureProjectHome";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  byTestId,
  sessionsDrawerItem,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectGroupHeader,
  sessionsDrawerProjectHome,
} from "../support/testIds";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };
const ALPHA_HOME: ProjectHomeInfo = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeAgentLabel: "Claude" };
const BETA_NO_AGENT: ProjectHomeInfo = { projectId: BETA.projectId, mainBranchRef: "origin/master", homeAgentLabel: null };

function aSession(sessionId: string, projectId: string, overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    projectId,
    createdAt: "2026-10-22T10:00:00Z",
    status: "active",
    isActive: true,
    pid: 4242,
    ...overrides,
  });
}

const ALPHA_HOME_SESSION = aSession("home-alpha", ALPHA.projectId, {
  sessionRole: "project-home",
  managedCodebase: true,
});

function aDrawer(sessions: SessionEntry[], onEnsure: (projectId: string) => Promise<void> = cy.stub().resolves()) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions}
        projects={[ALPHA, BETA]}
        projectHomes={[ALPHA_HOME, BETA_NO_AGENT]}
        onEnsureProjectHome={onEnsure}
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

/** The drawer wired to a daemon through `useEnsureProjectHome`, selecting what it ensures. */
function DrawerWithEnsure({ client, onSelected }: { client: ReturnType<typeof createClient<typeof SessionService>>; onSelected: (id: string) => void }) {
  const { ensure } = useEnsureProjectHome(client, "tok", onSelected);
  return (
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={[aSession("a1", ALPHA.projectId)]}
        projects={[ALPHA]}
        projectHomes={[ALPHA_HOME]}
        onEnsureProjectHome={(projectId) => ensure(projectId, "")}
        selectedSessionId={null}
        onSelectSession={cy.stub()}
        isOpen
        onClose={cy.stub()}
        onOpen={cy.stub()}
        crossHostSessionsVisible
      />
    </TooltipProvider>
  );
}

/** Test ids of the rows inside a project group, in DOM order. */
function rowOrderIn(projectId: string): Cypress.Chainable<string[]> {
  return byTestId(sessionsDrawerProjectGroup(projectId))
    .find("[data-testid^='sessions-drawer-project-home-'], [data-testid^='sessions-drawer-item-']")
    .then(($rows) =>
      [...$rows]
        .map((el) => el.getAttribute("data-testid")!)
        // A row's own parts (status dot, label) and the entry's error are not rows.
        .filter((id) => !/^sessions-drawer-(item-(status|label)|project-home-error)-/.test(id)),
    );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
});

it("each project group shows a pinned main entry first", () => {
  // When
  aDrawer([aSession("a1", ALPHA.projectId), aSession("b1", BETA.projectId)]);

  // Then
  rowOrderIn(ALPHA.projectId).its(0).should("equal", sessionsDrawerProjectHome(ALPHA.projectId));
  byTestId(sessionsDrawerProjectHome(ALPHA.projectId))
    .should("contain", "main")
    .and("contain", "origin/main")
    .and("contain", "Claude");
  rowOrderIn(BETA.projectId).its(0).should("equal", sessionsDrawerProjectHome(BETA.projectId));
});

it("clicking the pinned entry ensures the home session and selects it", () => {
  // Given a daemon whose ensure answers with the started home session
  const backend = anInMemoryRpcBackend().onUnary(SessionService.method.ensureProjectHomeSession, () => ({
    session: ALPHA_HOME_SESSION,
    outcome: "started",
  }));
  const onSelected = cy.stub().as("onSelected");
  mountWithRpc(
    <DrawerWithEnsure client={createClient(SessionService, backend.transport())} onSelected={onSelected} />,
    backend,
  );

  // When
  byTestId(sessionsDrawerProjectHome(ALPHA.projectId)).click();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(SessionService.method.ensureProjectHomeSession);
    expect(calls).to.have.length(1);
    expect(calls[0]).to.include({ projectId: ALPHA.projectId });
  });
  cy.get("@onSelected").should("have.been.calledOnceWith", ALPHA_HOME_SESSION.sessionId);
});

it("the home session is not repeated in its group's rows", () => {
  // When
  aDrawer([ALPHA_HOME_SESSION, aSession("a1", ALPHA.projectId)]);

  // Then
  rowOrderIn(ALPHA.projectId).should("deep.equal", [
    sessionsDrawerProjectHome(ALPHA.projectId),
    sessionsDrawerItem("a1"),
  ]);
});

it("a project without a home agent starts nothing and shows no hint", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  aDrawer([aSession("b1", BETA.projectId)], onEnsure);

  // When
  byTestId(sessionsDrawerProjectHome(BETA.projectId)).click();

  // Then
  byTestId(sessionsDrawerProjectGroup(BETA.projectId)).should("not.contain", "Projects screen");
  cy.get("@onEnsure").should("not.have.been.called");
});

it("the Other group has no pinned entry", () => {
  // When
  aDrawer([aSession("a1", ALPHA.projectId), aSession("orphan", "")]);

  // Then
  byTestId(sessionsDrawerProjectGroup("other"))
    .find("[data-testid^='sessions-drawer-project-home-']")
    .should("not.exist");
  byTestId(sessionsDrawerItem("orphan")).should("exist");
});

it("a project with a home agent and no session yet still shows its pinned entry", () => {
  // When only beta (no home agent) has a session
  aDrawer([aSession("b1", BETA.projectId)]);

  // Then alpha's group exists for its pinned entry alone, ordered before beta's
  rowOrderIn(ALPHA.projectId).should("deep.equal", [sessionsDrawerProjectHome(ALPHA.projectId)]);
  cy.get("[data-testid^='sessions-drawer-project-group-header-']").then(($headers) => {
    expect([...$headers].map((h) => h.getAttribute("data-testid"))).to.deep.equal([
      sessionsDrawerProjectGroupHeader(ALPHA.projectId),
      sessionsDrawerProjectGroupHeader(BETA.projectId),
    ]);
  });
});

it("a project with no home agent and no session gets no group", () => {
  // When only alpha has a session
  aDrawer([aSession("a1", ALPHA.projectId)]);

  // Then
  byTestId(sessionsDrawerProjectGroup(BETA.projectId)).should("not.exist");
});
