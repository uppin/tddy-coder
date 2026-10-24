/**
 * Acceptance: each project group in the drawer starts with a pinned **main** entry for the session
 * the project records as its main (`ProjectEntry.home_session_id`). Clicking a set-up main ensures
 * it (`EnsureProjectHomeSession`, no start spec) and selects it; main is not repeated among the
 * group's rows, and is labelled with its model; another session carrying the project-home role (a
 * replaced main) is an ordinary row; a project with no main starts nothing and shows no hint (its
 * click opens the setup pane); a set-up main offers **Replace main**; the **Other** group has no
 * pinned entry.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry.
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
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
  sessionsDrawerProjectHomeReplace,
} from "../support/testIds";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };
const ALPHA_HOME: ProjectHomeInfo = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeSessionId: "home-alpha" };
const BETA_NO_MAIN: ProjectHomeInfo = { projectId: BETA.projectId, mainBranchRef: "origin/master", homeSessionId: null };

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
  model: "opus",
});

/** A main that was replaced: it keeps its project-home role, but the project no longer records it. */
const ALPHA_REPLACED_MAIN = aSession("old-alpha", ALPHA.projectId, {
  sessionRole: "project-home",
  managedCodebase: true,
  isActive: false,
  status: "inactive",
});

function aDrawer(
  sessions: SessionEntry[],
  onEnsure: (projectId: string) => Promise<void> = cy.stub().resolves(),
  onReplaceProjectHome: (projectId: string) => void = cy.stub(),
) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions}
        projects={[ALPHA, BETA]}
        projectHomes={[ALPHA_HOME, BETA_NO_MAIN]}
        onEnsureProjectHome={onEnsure}
        onReplaceProjectHome={onReplaceProjectHome}
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
        // A row's own parts (status dot, label) and the entry's error and Replace action are not rows.
        .filter((id) => !/^sessions-drawer-(item-(status|label)|project-home-(error|replace))-/.test(id)),
    );
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
});

it("each project group shows a pinned main entry first, labelled with main's model", () => {
  // When
  aDrawer([ALPHA_HOME_SESSION, aSession("a1", ALPHA.projectId), aSession("b1", BETA.projectId)]);

  // Then
  rowOrderIn(ALPHA.projectId).its(0).should("equal", sessionsDrawerProjectHome(ALPHA.projectId));
  byTestId(sessionsDrawerProjectHome(ALPHA.projectId))
    .should("contain", "main")
    .and("contain", "origin/main")
    .and("contain", "opus");
  rowOrderIn(BETA.projectId).its(0).should("equal", sessionsDrawerProjectHome(BETA.projectId));
});

it("clicking a set-up main ensures it without a start spec and selects it", () => {
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
    expect(calls[0].start).to.equal(undefined);
  });
  cy.get("@onSelected").should("have.been.calledOnceWith", ALPHA_HOME_SESSION.sessionId);
});

it("main is not repeated in its group's rows", () => {
  // When
  aDrawer([ALPHA_HOME_SESSION, aSession("a1", ALPHA.projectId)]);

  // Then
  rowOrderIn(ALPHA.projectId).should("deep.equal", [
    sessionsDrawerProjectHome(ALPHA.projectId),
    sessionsDrawerItem("a1"),
  ]);
});

it("a project with no main starts nothing and shows no hint", () => {
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

it("a project whose main is not listed yet still shows its pinned entry", () => {
  // When only beta (no main) has a session
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

it("a project with no main and no session gets no group", () => {
  // When only alpha has a session
  aDrawer([aSession("a1", ALPHA.projectId)]);

  // Then
  byTestId(sessionsDrawerProjectGroup(BETA.projectId)).should("not.exist");
});

it("a session with the project-home role that is not the recorded main is an ordinary row", () => {
  // When
  aDrawer([ALPHA_HOME_SESSION, ALPHA_REPLACED_MAIN]);

  // Then
  rowOrderIn(ALPHA.projectId).should("deep.equal", [
    sessionsDrawerProjectHome(ALPHA.projectId),
    sessionsDrawerItem(ALPHA_REPLACED_MAIN.sessionId),
  ]);
});

it("a set-up main offers Replace main; a project with no main does not", () => {
  // When
  aDrawer([ALPHA_HOME_SESSION, aSession("b1", BETA.projectId)]);

  // Then
  byTestId(sessionsDrawerProjectHomeReplace(ALPHA.projectId)).should("exist");
  byTestId(sessionsDrawerProjectHomeReplace(BETA.projectId)).should("not.exist");
});

it("Replace main asks to set that project's main up again, and starts nothing", () => {
  // Given
  const onEnsure = cy.stub().as("onEnsure").resolves();
  const onReplace = cy.stub().as("onReplace");
  aDrawer([ALPHA_HOME_SESSION], onEnsure, onReplace);

  // When
  byTestId(sessionsDrawerProjectHomeReplace(ALPHA.projectId)).click();

  // Then
  cy.get("@onReplace").should("have.been.calledOnceWith", ALPHA.projectId);
  cy.get("@onEnsure").should("not.have.been.called");
});
