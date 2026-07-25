/**
 * Acceptance tests: the PR-Stack Chat Screen's live status & repoint.
 *
 * A planned-PR row resolves its in-progress session by branch, shows the GitHub PR number (as a
 * link) and state polled from `GetPrStatus`, and offers a Repoint control when a predecessor has
 * merged — wired to `RepointPlannedPr`.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md. Changeset: docs/dev/1-WIP/pr-stack-live-status.md.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-2222-0000-0000-0000-000000000020";
const PROJECT_ID = "proj-pr-stack";

/** The interval (ms) at which the PR-Stack view re-polls `GetPrStatus`. */
const POLL_INTERVAL_MS = 5000;

interface PrStatusFixture {
  exists: boolean;
  number?: number;
  url?: string;
  state?: string;
}

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    projectId: PROJECT_ID,
    recipe: "pr-stack",
    stackPlanJson,
  };
}

/** A live claude-cli child session working `branch`, nested under the orchestrator. */
function aChildSessionOnBranch(branch: string, sessionId: string): Partial<SessionEntry> {
  return {
    sessionId,
    createdAt: "2026-07-01T09:10:00Z",
    status: "active",
    repoPath: "/home/dev/pr-stack-project",
    isActive: true,
    projectId: PROJECT_ID,
    recipe: "tdd",
    sessionType: "claude-cli",
    orchestratorSessionId: ORCHESTRATOR_SESSION_ID,
    branch,
  };
}

interface MountOptions {
  sessions: Partial<SessionEntry>[];
  /** Static `GetPrStatus` result per branch. */
  prStatusByBranch?: Record<string, PrStatusFixture>;
  /** Dynamic `GetPrStatus` result per branch, re-evaluated on every poll (polling tests). */
  prStatusFactory?: (branch: string) => PrStatusFixture;
  /** `RepointPlannedPr` response stack JSON. */
  repointResponseStackJson?: string;
}

function toStatusResponse(fx: PrStatusFixture) {
  return {
    status: {
      exists: fx.exists,
      number: BigInt(fx.number ?? 0),
      url: fx.url ?? "",
      state: fx.state ?? "",
    },
  };
}

function openPrStackScreen(opts: MountOptions) {
  const repointSpy = cy.stub().as("repointPlannedPr");
  const backend = aSessionsDrawerBackend(opts.sessions)
    .onUnary(ConnectionService.method.getPrStatus, (req: { branch: string }) => {
      const fx = opts.prStatusFactory
        ? opts.prStatusFactory(req.branch)
        : (opts.prStatusByBranch?.[req.branch] ?? { exists: false });
      return toStatusResponse(fx);
    })
    .onUnary(ConnectionService.method.repointPlannedPr, (req: { nodeId: string }) => {
      repointSpy(req);
      return { stackPlanJson: opts.repointResponseStackJson ?? "" };
    });
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
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
// In-progress resolution by branch
// ---------------------------------------------------------------------------

it("marks a planned-PR row in progress when a live session owns its branch", () => {
  // Given — node n1 owns feature/x/n1, and a live child session works that same branch
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [
      anOrchestratorSession(plan),
      aChildSessionOnBranch("feature/x/n1", "child-n1"),
    ],
  });

  // Then
  prStackScreenPage.inProgressBadge("n1").should("exist");
});

it("shows no in-progress indicator when no session owns the node branch", () => {
  // Given — node n1 owns feature/x/n1, but the only other session works a different branch
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [
      anOrchestratorSession(plan),
      aChildSessionOnBranch("feature/x/other", "child-other"),
    ],
  });

  // Then
  prStackScreenPage.inProgressBadge("n1").should("not.exist");
});

// ---------------------------------------------------------------------------
// GitHub PR status (number, link, state)
// ---------------------------------------------------------------------------

it("shows the GitHub PR number as a link to the PR for the node branch", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    prStatusByBranch: {
      "feature/x/n1": {
        exists: true,
        number: 42,
        url: "https://github.com/acme/repo/pull/42",
        state: "open",
      },
    },
  });

  // Then
  prStackScreenPage
    .prLink("n1")
    .should("contain.text", "#42")
    .and("have.attr", "href", "https://github.com/acme/repo/pull/42");
});

it("shows the GitHub PR state reported for the node branch", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);

  // When
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    prStatusByBranch: {
      "feature/x/n1": {
        exists: true,
        number: 42,
        url: "https://github.com/acme/repo/pull/42",
        state: "merged",
      },
    },
  });

  // Then
  prStackScreenPage.prState("n1").should("contain.text", "merged");
});

it("updates the PR status on the polling interval without user action", () => {
  // Given — the PR is open on the first poll and merged on every subsequent poll
  cy.clock();
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", branch: "feature/x/n1" }),
  ]);
  let polls = 0;
  openPrStackScreen({
    sessions: [anOrchestratorSession(plan)],
    prStatusFactory: () => {
      polls += 1;
      return {
        exists: true,
        number: 42,
        url: "https://github.com/acme/repo/pull/42",
        state: polls === 1 ? "open" : "merged",
      };
    },
  });

  // Then — the first poll shows open
  prStackScreenPage.prState("n1").should("contain.text", "open");

  // When — the poll interval elapses
  cy.tick(POLL_INTERVAL_MS);

  // Then — the row reflects the newer poll without any user action
  prStackScreenPage.prState("n1").should("contain.text", "merged");
});

// ---------------------------------------------------------------------------
// Repoint control
// ---------------------------------------------------------------------------

it("shows a Repoint control on a node whose predecessor has merged", () => {
  // Given — n1 has merged; n2 depends on n1
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/x/n1",
      sessionId: "child-n1",
      prStatus: { phase: "merged" },
    }),
    aPlannedNode({ nodeId: "n2", title: "Add middleware", branch: "feature/x/n2", parents: ["n1"] }),
  ]);

  // When
  openPrStackScreen({ sessions: [anOrchestratorSession(plan)] });

  // Then
  prStackScreenPage.repointBtn("n2").should("exist");
});

it("hides the Repoint control when no predecessor has merged", () => {
  // Given — n1 is still open; n2 depends on n1
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/x/n1",
      sessionId: "child-n1",
      prStatus: { phase: "open" },
    }),
    aPlannedNode({ nodeId: "n2", title: "Add middleware", branch: "feature/x/n2", parents: ["n1"] }),
  ]);

  // When
  openPrStackScreen({ sessions: [anOrchestratorSession(plan)] });

  // Then
  prStackScreenPage.repointBtn("n2").should("not.exist");
});

it("repoints the node via RepointPlannedPr when the Repoint control is clicked", () => {
  // Given — n1 merged, n2 depends on n1; after repoint n2 no longer depends on n1
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/x/n1",
      sessionId: "child-n1",
      prStatus: { phase: "merged" },
    }),
    aPlannedNode({ nodeId: "n2", title: "Add middleware", branch: "feature/x/n2", parents: ["n1"] }),
  ]);
  const repointed = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/x/n1",
      sessionId: "child-n1",
      prStatus: { phase: "merged" },
    }),
    aPlannedNode({ nodeId: "n2", title: "Add middleware", branch: "feature/x/n2", parents: [] }),
  ]);

  // When
  openPrStackScreen({ sessions: [anOrchestratorSession(plan)], repointResponseStackJson: repointed });
  prStackScreenPage.clickRepoint("n2");

  // Then — RepointPlannedPr is called for n2 and the row's Repoint control clears from the response
  cy.get("@repointPlannedPr").should("have.been.calledWithMatch", { nodeId: "n2" });
  prStackScreenPage.repointBtn("n2").should("not.exist");
});
