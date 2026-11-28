/**
 * Acceptance tests: the PR-Stack view stops asking `QueryBranch` when nobody is looking or nothing
 * changes.
 *
 * Each `QueryBranch` costs the daemon a session scan, a git comparison on its blocking lane and a PR
 * lookup. A desktop window left open on a three-branch stack polled every 5 s overnight: ~2,200 lookups
 * an hour, 29k blocking-lane refusals and an 81 MB log, with nobody at the machine.
 *
 * - A hidden page polls nothing, and polls again at once when it is shown.
 * - Answers that keep coming back unchanged stretch the wait: 5 s, then 15 s after three unchanged
 *   answers in a row, 30 s after six, 60 s after twelve. A changed answer goes back to 5 s.
 */

import { WorkQueueService, type QueryBranchResponse } from "tddy-client-web/gen/work_queue_pb";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { openProjectStackView } from "../support/rpc/openProjectStackView";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import {
  aPlannedNode,
  aWorkQueueJson,
  aBranchResolutionResponse,
  type BranchResolutionFixture,
} from "../support/rpc/prStackFixtures";

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-4444-0000-0000-0000-000000000077";
const POLL_INTERVAL_MS = 5000;

const plan = aWorkQueueJson(1, [
  aPlannedNode({ itemId: "n1", title: "Add token store", branch: "feature/x/n1" }),
]);

/** Opens the stack view; every `QueryBranch` is counted and answered by `answer`. */
function openStackViewCounting(answer: (poll: number) => BranchResolutionFixture) {
  const calls = { count: 0 };
  // The fixture builds the response's shape without the protobuf envelope; the in-memory router
  // answers from the plain object, so the cast states what the wire would carry.
  const backend = aSessionsDrawerBackend([]).onUnary(WorkQueueService.method.queryBranch, () => {
    calls.count += 1;
    return aBranchResolutionResponse(answer(calls.count)) as unknown as QueryBranchResponse;
  });
  openProjectStackView(backend, plan, ORCHESTRATOR_SESSION_ID);
  return calls;
}

function expectCalls(calls: { count: number }, expected: number, why: string) {
  cy.wrap(null).should(() => expect(calls.count, why).to.equal(expected));
}

/** Makes the page report `state` and fires `visibilitychange`, as a minimised window does. */
function setVisibility(state: "hidden" | "visible") {
  cy.document().then((doc) => {
    Object.defineProperty(doc, "visibilityState", { configurable: true, get: () => state });
    Object.defineProperty(doc, "hidden", { configurable: true, get: () => state === "hidden" });
    doc.dispatchEvent(new Event("visibilitychange"));
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  // Only the poll's timers are faked (see PrStackBranchQueryAcceptance for why).
  cy.clock(Date.now(), ["setInterval", "clearInterval"]);
});

afterEach(() => {
  setVisibility("visible");
});

it("polls nothing while the page is hidden, and polls at once when it is shown again", () => {
  // Given — the view has made its first poll
  const calls = openStackViewCounting(() => ({ branch: "feature/x/n1" }));
  prStackScreenPage.workItemRow("n1").should("exist");
  expectCalls(calls, 1, "the first poll on mount");

  // When — the window is hidden for ten minutes
  setVisibility("hidden");
  cy.tick(10 * 60_000);

  // Then — not one QueryBranch was sent
  expectCalls(calls, 1, "polls while hidden");

  // When — the window is shown again
  setVisibility("visible");

  // Then — the view catches up immediately, without waiting out an interval
  expectCalls(calls, 2, "the catch-up poll when shown");
});

it("waits longer between polls while the answers do not change", () => {
  // Given — a branch whose resolution never changes
  const calls = openStackViewCounting(() => ({ branch: "feature/x/n1" }));
  prStackScreenPage.workItemRow("n1").should("exist");
  expectCalls(calls, 1, "the first poll on mount");

  // When — three more polls come back unchanged at the base interval
  for (let i = 2; i <= 4; i += 1) {
    cy.tick(POLL_INTERVAL_MS);
    expectCalls(calls, i, `poll ${i} at the base interval`);
  }

  // Then — the next poll waits 15 s, not 5 s
  cy.tick(POLL_INTERVAL_MS);
  expectCalls(calls, 4, "no poll 5 s after the third unchanged answer");
  cy.tick(15_000 - POLL_INTERVAL_MS);
  expectCalls(calls, 5, "the poll 15 s after the third unchanged answer");
});

it("returns to the base interval as soon as an answer changes", () => {
  // Given — a quiet branch that gains a worktree on the fifth poll
  const calls = openStackViewCounting((poll) =>
    poll < 5
      ? { branch: "feature/x/n1", worktree: { exists: false } }
      : { branch: "feature/x/n1", worktree: { exists: true, path: "/home/dev/wt/n1" } },
  );
  prStackScreenPage.workItemRow("n1").should("exist");
  expectCalls(calls, 1, "the first poll on mount");
  for (let i = 2; i <= 4; i += 1) {
    cy.tick(POLL_INTERVAL_MS);
    expectCalls(calls, i, `poll ${i}`);
  }

  // When — the backed-off poll answers with a change
  cy.tick(15_000);
  expectCalls(calls, 5, "the backed-off poll");
  prStackScreenPage.worktree("n1").should("contain.text", "/home/dev/wt/n1");

  // Then — the following poll is back at 5 s
  cy.tick(POLL_INTERVAL_MS);
  expectCalls(calls, 6, "the poll 5 s after a changed answer");
});
