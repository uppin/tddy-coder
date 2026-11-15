/**
 * Acceptance: a session whose worktree lives on a different daemon than its agent renders both
 * placements, so an operator can tell at a glance where the code actually is.
 *
 * A split session is the only case where "which host is this session on?" has two answers, and
 * neither can be inferred from which daemon answered `ListSessions` — the placement is carried
 * explicitly on `SessionEntry` as `daemonInstanceId` (the agent) and `codebaseDaemonInstanceId`
 * (the worktree).
 *
 * Mounted through `SessionDrawer` rather than `SessionDrawerItem` so the component derives its own
 * badge labels from the entries. Passing the labels in as props and then asserting on them would
 * compare the fixture against itself and leave `badgeCodebaseHostLabel` — where the behaviour
 * actually lives — untested.
 *
 * PRD: docs/ft/daemon/remote-managed-worktree.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { SessionDrawer } from "../../src/SessionDrawer";
import { TooltipProvider } from "tddy-ui-web";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const AGENT_HOST = "laptop-a";
const CODEBASE_HOST = "workstation-b";

/** Human labels the drawer resolves instance ids through, as the real screen supplies them. */
const HOST_LABELS: Record<string, string> = {
  [AGENT_HOST]: "laptop-a (this daemon)",
  [CODEBASE_HOST]: "workstation-b",
};

const SPLIT_SESSION_ID = "aaaaaaaa-0000-4000-8000-00000000000a";
const COLOCATED_SESSION_ID = "bbbbbbbb-0000-4000-8000-00000000000b";

/**
 * A session whose agent runs on `laptop-a` while its worktree lives on `workstation-b`.
 * `repoPath` is empty because there is no repository on the agent's host at all.
 */
function aSplitSession() {
  return create(SessionEntrySchema, {
    sessionId: SPLIT_SESSION_ID,
    createdAt: "2026-08-13T12:00:00Z",
    status: "active",
    repoPath: "",
    pid: 41001,
    isActive: true,
    projectId: "proj-1",
    sessionType: "claude-cli",
    daemonInstanceId: AGENT_HOST,
    codebaseDaemonInstanceId: CODEBASE_HOST,
    codebaseSessionId: "cccccccc-0000-4000-8000-00000000000c",
    pendingElicitation: false,
  });
}

/** An ordinary session: agent and worktree on the same daemon. */
function aCoLocatedSession() {
  return create(SessionEntrySchema, {
    sessionId: COLOCATED_SESSION_ID,
    createdAt: "2026-08-13T12:00:00Z",
    status: "active",
    repoPath: "/home/dev/repo/.worktrees/feature",
    pid: 41002,
    isActive: true,
    projectId: "proj-1",
    sessionType: "claude-cli",
    daemonInstanceId: AGENT_HOST,
    codebaseDaemonInstanceId: "",
    codebaseSessionId: "",
    pendingElicitation: false,
  });
}

/** The drawer, with `laptop-a` selected — so the agent host is the *unremarkable* one. */
function mountDrawerWith(sessions: ReturnType<typeof aSplitSession>[]) {
  cy.mount(
    <TooltipProvider delayDuration={0}>
      <SessionDrawer
        sessions={sessions as Parameters<typeof SessionDrawer>[0]["sessions"]}
        selectedSessionId={null}
        onSelectSession={cy.stub()}
        isOpen
        onClose={cy.stub()}
        onOpen={cy.stub()}
        crossHostSessionsVisible
        selectedInstanceId={AGENT_HOST}
        hostLabelForInstance={(instanceId: string) => HOST_LABELS[instanceId] ?? instanceId}
      />
    </TooltipProvider>,
  );
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("badges the codebase host of a split session with its resolved label", () => {
  // Given a session running on the selected host against a worktree on another
  mountDrawerWith([aSplitSession()]);

  // Then the drawer resolves the codebase instance id through the same label mapping the agent
  // host uses — an operator reads a host name here, not a raw instance id
  sessionsDrawerPage
    .codebaseHostBadge(SPLIT_SESSION_ID)
    .should("have.text", HOST_LABELS[CODEBASE_HOST]);
});

it("badges the codebase host only on the session that actually has one", () => {
  // Given a split session and an ordinary co-located one side by side
  mountDrawerWith([aSplitSession(), aCoLocatedSession()]);

  // Then only the split row carries the badge. An unconditional one would label every pre-existing
  // session as if its codebase were somewhere else.
  sessionsDrawerPage
    .codebaseHostBadge(SPLIT_SESSION_ID)
    .should("have.text", HOST_LABELS[CODEBASE_HOST]);
  sessionsDrawerPage.expectNoCodebaseHostBadge(COLOCATED_SESSION_ID);
});

it("badges the codebase host even when the session's agent runs on the selected host", () => {
  // Given a split session whose agent is on the selected host, so its *agent* host is unremarkable
  // and carries no owning-host badge
  mountDrawerWith([aSplitSession()]);

  // Then the codebase badge still renders: unlike the agent's host, where the worktree lives cannot
  // be inferred from the row appearing in this drawer at all
  sessionsDrawerPage.expectNoOwningHostBadge(SPLIT_SESSION_ID);
  sessionsDrawerPage
    .codebaseHostBadge(SPLIT_SESSION_ID)
    .should("have.text", HOST_LABELS[CODEBASE_HOST]);
});

it("shows the branch a split session's codebase works on beside its codebase host", () => {
  // Given a split session whose codebase half works on `test-split-udoo-4`, beside a co-located
  // session on its own branch
  mountDrawerWith([
    create(SessionEntrySchema, { ...aSplitSession(), branch: "test-split-udoo-4" }),
    create(SessionEntrySchema, { ...aCoLocatedSession(), branch: "feature" }),
  ]);

  // Then the split row names its branch — the agent half has no worktree of its own to name it
  // by — and the co-located row, labelled by its worktree, carries no such slot
  sessionsDrawerPage.splitBranch(SPLIT_SESSION_ID).should("have.text", "test-split-udoo-4");
  sessionsDrawerPage.expectNoSplitBranch(COLOCATED_SESSION_ID);
});

it("shows no branch slot on a split session that has not recorded one", () => {
  // Given a split session started before the agent half recorded its codebase branch
  mountDrawerWith([aSplitSession()]);

  // Then the row shows its codebase host and no empty branch slot
  sessionsDrawerPage
    .codebaseHostBadge(SPLIT_SESSION_ID)
    .should("have.text", HOST_LABELS[CODEBASE_HOST]);
  sessionsDrawerPage.expectNoSplitBranch(SPLIT_SESSION_ID);
});
