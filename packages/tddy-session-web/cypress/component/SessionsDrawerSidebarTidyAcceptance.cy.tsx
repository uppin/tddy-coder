/**
 * Acceptance: the tidied sessions drawer — projects with live sessions pinned first, one Stacks box
 * above Select opening an upward drawer of stacks grouped per project (named, not by UUID), no count
 * beside a project's name, and a settings cog left of the name that shows on hovering the header row.
 *
 * PRD: docs/ft/web/session-drawer.md
 */

import React from "react";
import "tddy-ui-web/theme.css";
import { create } from "@bufbuild/protobuf";
import type { Client } from "@connectrpc/connect";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import {
  WorkQueueSummarySchema,
  type WorkQueueService,
  type WorkQueueSummary,
} from "tddy-client-web/gen/work_queue_pb";
import { TooltipProvider } from "tddy-ui-web";
import { SessionDrawer } from "../../src/SessionDrawer";
import {
  byTestId,
  sessionsDrawerProjectGroup,
  sessionsDrawerProjectGroupHeader,
  sessionsDrawerProjectStack,
  sessionsDrawerProjectStacks,
} from "../support/testIds";

const ALPHA = { projectId: "proj-alpha", name: "alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };
const GAMMA = { projectId: "proj-gamma", name: "gamma" };
const STACK_OWNER = "0190b000-0000-7000-8000-0000000000aa";

function aSession(sessionId: string, projectId: string, live: boolean, displayName = ""): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId,
    projectId,
    displayName,
    createdAt: "2026-10-22T10:00:00Z",
    status: live ? "active" : "exited",
    isActive: live,
    pid: live ? 4242 : 0,
  });
}

function aStack(projectId: string, queueId: string, title: string): WorkQueueSummary {
  return create(WorkQueueSummarySchema, { address: { projectId, queueId }, title, nodeCounts: { total: 2 } });
}

function aClientListing(stacks: Record<string, WorkQueueSummary[]>) {
  return {
    listProjectWorkQueues: async ({ projectId }: { projectId: string }) => ({ stacks: stacks[projectId] ?? [] }),
  } as unknown as Client<typeof WorkQueueService>;
}

function mountDrawer(opts: {
  sessions: SessionEntry[];
  stacks?: Record<string, WorkQueueSummary[]>;
  onOpenProjectStack?: (s: WorkQueueSummary) => void;
  onOpenProjectSettings?: (projectId: string) => void;
}) {
  cy.mount(
    <TooltipProvider>
      <div style={{ height: 700, display: "flex" }}>
        <SessionDrawer
          sessions={opts.sessions}
          projects={[ALPHA, BETA, GAMMA]}
          selectedSessionId={null}
          onSelectSession={() => {}}
          isOpen
          onClose={() => {}}
          onOpen={() => {}}
          crossHostSessionsVisible
          onNewOrchestrator={() => {}}
          onOpenProjectSettings={opts.onOpenProjectSettings ?? (() => {})}
          workQueueClient={aClientListing(opts.stacks ?? {})}
          onOpenProjectStack={opts.onOpenProjectStack ?? (() => {})}
          onEnterSelectionMode={() => {}}
        />
      </div>
    </TooltipProvider>,
  );
}

const groupOrder = (): Cypress.Chainable<string[]> =>
  cy
    .get("[data-testid^='sessions-drawer-project-group-header-']")
    .then(($h) => [...$h].map((el) => el.getAttribute("data-testid")!.replace("sessions-drawer-project-group-header-", "")));

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
});

it("projects with active sessions are pinned above the rest, each side in name order", () => {
  // Given — only gamma and beta have a live session
  mountDrawer({
    sessions: [
      aSession("a1", ALPHA.projectId, false),
      aSession("b1", BETA.projectId, true),
      aSession("g1", GAMMA.projectId, true),
      aSession("orphan", "", true),
    ],
  });

  // Then
  groupOrder().should("deep.equal", [BETA.projectId, GAMMA.projectId, ALPHA.projectId, "other"]);
});

it("a project header shows no session count", () => {
  // Given
  mountDrawer({ sessions: [aSession("a1", ALPHA.projectId, true), aSession("a2", ALPHA.projectId, true)] });

  // Then
  byTestId(sessionsDrawerProjectGroupHeader(ALPHA.projectId)).invoke("text").should("equal", "alpha");
});

it("stacks live in one box above Select that opens upward, grouped per project with readable names", () => {
  // Given — alpha's untitled stack is owned by a named session; beta's stack has a title
  const opened: WorkQueueSummary[] = [];
  mountDrawer({
    sessions: [aSession(STACK_OWNER, ALPHA.projectId, true, "Auth rework"), aSession("b1", BETA.projectId, false)],
    stacks: {
      [ALPHA.projectId]: [aStack(ALPHA.projectId, STACK_OWNER, "")],
      [BETA.projectId]: [aStack(BETA.projectId, "q-beta", "Billing")],
    },
    onOpenProjectStack: (s) => opened.push(s),
  });

  // Then — nothing about stacks inside a project group
  byTestId(sessionsDrawerProjectGroup(ALPHA.projectId)).find("[data-testid^='sessions-drawer-project-queue']").should("not.exist");
  byTestId("sessions-drawer-stacks-box").should("be.visible").and("contain", "Stacks");
  byTestId("sessions-drawer-stacks-panel").should("not.exist");
  cy.get("[data-testid='sessions-drawer-stacks-box']").then(($box) => {
    cy.get("[data-testid='sessions-drawer-select-bar']").then(($bar) => {
      expect($box[0].getBoundingClientRect().bottom).to.be.at.most($bar[0].getBoundingClientRect().top + 1);
    });
  });

  // When
  byTestId("sessions-drawer-stacks-box").click();

  // Then — the panel sits above the box, grouped per project, named
  byTestId("sessions-drawer-stacks-panel").should("be.visible").then(($panel) => {
    const box = Cypress.$("[data-testid='sessions-drawer-stacks-box']")[0].getBoundingClientRect();
    expect($panel[0].getBoundingClientRect().bottom).to.be.at.most(box.top + 1);
  });
  byTestId(sessionsDrawerProjectStacks(ALPHA.projectId)).should("contain", "alpha").and("contain", "Auth rework");
  byTestId(sessionsDrawerProjectStacks(ALPHA.projectId)).should("not.contain", STACK_OWNER);
  byTestId(sessionsDrawerProjectStacks(BETA.projectId)).should("contain", "beta").and("contain", "Billing");

  // When
  byTestId(sessionsDrawerProjectStack("q-beta")).click();

  // Then
  cy.wrap(opened).should("have.length", 1).its("0.address.queueId").should("equal", "q-beta");
});

it("no Stacks box when no project has a stack", () => {
  mountDrawer({ sessions: [aSession("a1", ALPHA.projectId, true)] });
  byTestId(sessionsDrawerProjectGroupHeader(ALPHA.projectId)).should("exist");
  byTestId("sessions-drawer-stacks-box").should("not.exist");
});

it("the settings cog sits right of the project name and stays visible across the header row", () => {
  // Given
  const openedSettings: string[] = [];
  mountDrawer({ sessions: [aSession("a1", ALPHA.projectId, true)], onOpenProjectSettings: (p) => openedSettings.push(p) });
  const cog = () => byTestId(`sessions-drawer-project-settings-${ALPHA.projectId}`);
  const row = () => byTestId(`sessions-drawer-project-header-row-${ALPHA.projectId}`);

  // Then — hidden until the row is hovered
  cog().should("have.css", "opacity", "0");
  cog().then(($cog) => {
    const name = Cypress.$(`[data-testid='${sessionsDrawerProjectGroupHeader(ALPHA.projectId)}'] span`).filter(
      (_, el) => el.textContent === "alpha",
    )[0];
    expect($cog[0].getBoundingClientRect().left).to.be.at.least(name.getBoundingClientRect().right);
  });

  // And — the hover target is the whole row, which contains the cog, so moving from the name onto
  // the cog never leaves it (CSS :hover cannot be synthesised; the group wiring is what is pinned)
  row().should("match", ".group");
  row().within(() => cog().should("exist"));
  cog().should("have.class", "group-hover:opacity-100");

  // And clicking it
  cog().click({ force: true });

  // Then
  cy.wrap(openedSettings).should("deep.equal", [ALPHA.projectId]);
});

it("More sits on the right with its count, and project names line up with the session dots", () => {
  // Given — alpha has one live and one finished session
  mountDrawer({ sessions: [aSession("a-live", ALPHA.projectId, true), aSession("a-done", ALPHA.projectId, false)] });

  cy.get("[data-testid='sessions-drawer-item-a-live']").then(($row) => {
    const dot = $row.find("span").first()[0].getBoundingClientRect();
    const dotCentre = dot.left + dot.width / 2;
    const rowRight = $row[0].getBoundingClientRect().right;

    // Then — "More (1)" is right-aligned, its arrow at the row's right side
    cy.get("[data-testid='sessions-drawer-separator-remaining']").then(($more) => {
      expect($more.text()).to.equal("More (1)");
      const arrow = $more.find("svg")[0].getBoundingClientRect();
      expect(arrow.right).to.be.closeTo(rowRight - 12, 2);
      expect($more.find("span")[0].getBoundingClientRect().left).to.be.greaterThan(dotCentre + 100);
    });
    // And — the project's name starts where the session dot does
    byTestId(sessionsDrawerProjectGroupHeader(ALPHA.projectId))
      .find("span")
      .then(($name) => expect($name[0].getBoundingClientRect().left).to.be.closeTo(dot.left, 1));
  });
});

it("a project-home session carries a home icon; an orchestrator named home does not", () => {
  // Given — a replaced main (project-home role) and an orchestrator that happens to be called "home"
  const oldMain = { ...aSession("old-main", ALPHA.projectId, true, "home"), sessionRole: "project-home" };
  const orch = { ...aSession("orch", ALPHA.projectId, true, "home"), sessionRole: "orchestrator" };
  mountDrawer({ sessions: [oldMain, orch] });

  // Then
  byTestId("sessions-drawer-home-icon-old-main").should("be.visible");
  byTestId("sessions-drawer-item-orchestrator-orch").should("exist");
  byTestId("sessions-drawer-home-icon-orch").should("not.exist");
});
