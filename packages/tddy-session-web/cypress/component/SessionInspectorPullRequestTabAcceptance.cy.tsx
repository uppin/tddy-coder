/**
 * Acceptance (`#workqueue` 4/14): a session's inspector shows its PR and opens on it.
 *
 * Default tab: PR when the session's branch has a PR (`SessionEntry.prNumber`), otherwise Worktree; an
 * explicit `?inspector=` still wins. The PR tab streams `WorktreeService.StreamSessionPullRequest` — the
 * daemon reads GitHub through its inbox; the browser never does.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-22-session-pr-tabs.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import {
  PullRequestCheckSchema,
  PullRequestCommentSchema,
  SessionPullRequestViewSchema,
  WorktreeService,
} from "tddy-client-web/gen/worktree_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { appLocationPage } from "../support/pages/appLocationPage";

function aSession(sessionId: string, prNumber: bigint) {
  return {
    sessionId,
    createdAt: "2026-11-22T09:00:00Z",
    status: "idle",
    repoPath: `/home/dev/${sessionId}`,
    pid: 0,
    isActive: false,
    projectId: "proj-pr-tab",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    sessionType: "claude-cli",
    branch: "feat/one",
    prNumber,
    prState: prNumber > 0n ? "open" : "",
  };
}

const SESSION_WITH_PR = aSession("pr-tab-with-pr-0000-0000-000000000001", 7n);
const SESSION_WITHOUT_PR = aSession("pr-tab-no-pr-0000-0000-000000000002", 0n);

/** PR #7: a running build and a passed lint, one review thread with a reply, one conversation comment. */
const THE_PR_VIEW = create(SessionPullRequestViewSchema, {
  exists: true,
  number: 7n,
  url: "https://github.com/acme/app/pull/7",
  title: "Add the thing",
  state: "open",
  baseBranch: "master",
  headBranch: "feat/one",
  checks: [
    create(PullRequestCheckSchema, {
      name: "build",
      status: "in_progress",
      startedAt: new Date(Date.now() - 90_000).toISOString(),
      detailsUrl: "https://github.com/acme/app/runs/1",
    }),
    create(PullRequestCheckSchema, {
      name: "lint",
      status: "completed",
      conclusion: "success",
      startedAt: "2026-11-22T10:00:00Z",
      completedAt: "2026-11-22T10:01:00Z",
      detailsUrl: "https://github.com/acme/app/runs/2",
    }),
  ],
  comments: [
    create(PullRequestCommentSchema, { id: 41n, author: "rev", body: "why?", path: "a.rs" }),
    create(PullRequestCommentSchema, { id: 42n, inReplyToId: 41n, author: "dev", body: "because", path: "a.rs" }),
    create(PullRequestCommentSchema, { id: 51n, author: "ops", body: "ship it" }),
  ],
});

function aBackendServing(sessions: ReturnType<typeof aSession>[]) {
  return aSessionsDrawerBackend(sessions).implement(WorktreeService, {
    streamSessionPullRequest: async function* () {
      yield THE_PR_VIEW;
      await new Promise<never>(() => undefined);
    },
  });
}

function openInspectorOf(sessionId: string, sessions: ReturnType<typeof aSession>[]) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aBackendServing(sessions));
  sessionsDrawerPage.drawerItem(sessionId).click();
  sessionsDrawerPage.inspectorToggle().click();
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  appLocationPage.reset();
});

it("a session with a PR opens its inspector on the PR tab", () => {
  // When
  openInspectorOf(SESSION_WITH_PR.sessionId, [SESSION_WITH_PR]);

  // Then
  sessionsDrawerPage.inspectorPrTab().should("have.attr", "aria-selected", "true");
  sessionsDrawerPage.prTabPanel().should("be.visible");
});

it("a session without a PR opens its inspector on the Worktree tab", () => {
  // When
  openInspectorOf(SESSION_WITHOUT_PR.sessionId, [SESSION_WITHOUT_PR]);

  // Then
  sessionsDrawerPage.inspectorWorktreeTab().should("have.attr", "aria-selected", "true");
});

it("an inspector param naming details still opens Details", () => {
  // Given
  appLocationPage.startAt(`/sessions/${SESSION_WITH_PR.sessionId}?inspector=details`);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aBackendServing([SESSION_WITH_PR]));

  // Then
  sessionsDrawerPage.inspectorDetailsTab().should("have.attr", "aria-selected", "true");
  sessionsDrawerPage.inspectorPrTab().should("exist");
});

it("checks show running with elapsed time and completed with conclusion", () => {
  // When
  openInspectorOf(SESSION_WITH_PR.sessionId, [SESSION_WITH_PR]);

  // Then
  sessionsDrawerPage.prCheck("build").should("contain.text", "Running").and("contain.text", "1m");
  sessionsDrawerPage.prCheck("lint").should("contain.text", "success");
});

it("review comments are threaded under their first comment", () => {
  // When
  openInspectorOf(SESSION_WITH_PR.sessionId, [SESSION_WITH_PR]);

  // Then
  sessionsDrawerPage.prThread(41).within(() => {
    cy.get('[data-testid="session-pr-comment-41"]').should("exist");
    cy.get('[data-testid="session-pr-comment-42"]').should("exist");
  });
  sessionsDrawerPage.prComment(51).should("exist");
});
