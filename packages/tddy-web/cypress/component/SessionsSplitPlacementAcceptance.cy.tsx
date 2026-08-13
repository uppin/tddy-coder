/**
 * Acceptance: a session whose worktree lives on a different daemon than its agent renders both
 * placements, so an operator can tell at a glance where the code actually is.
 *
 * A split session is the only case where "which host is this session on?" has two answers, and
 * neither can be inferred from which daemon answered `ListSessions` — the placement is carried
 * explicitly on `SessionEntry` as `daemonInstanceId` (the agent) and `codebaseDaemonInstanceId`
 * (the worktree).
 *
 * PRD: docs/ft/daemon/remote-managed-worktree.md.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "../../src/gen/connection_pb";
import { SessionDrawerItem } from "../../src/components/sessions/SessionDrawerItem";
import { TooltipProvider } from "../../src/components/ui/tooltip";
import { byTestId, sessionsDrawerItemCodebaseHost, sessionsDrawerItemHost } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const AGENT_HOST = "laptop-a";
const CODEBASE_HOST = "workstation-b";

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

function aRow(session: ReturnType<typeof aSplitSession>) {
  return (
    <SessionDrawerItem
      key={session.sessionId}
      session={session}
      isSelected={false}
      onClick={cy.stub()}
      hostLabel={AGENT_HOST}
      codebaseHostLabel={session.codebaseDaemonInstanceId || null}
    />
  );
}

function mountRows(sessions: ReturnType<typeof aSplitSession>[]) {
  cy.mount(<TooltipProvider>{sessions.map(aRow)}</TooltipProvider>);
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

it("shows both the agent host and the codebase host for a split session", () => {
  // Given a session running on the laptop against a worktree on the workstation
  mountRows([aSplitSession()]);

  // Then both placements are named — one badge per host, each carrying its own instance id
  byTestId(sessionsDrawerItemHost(SPLIT_SESSION_ID)).should("have.text", AGENT_HOST);
  byTestId(sessionsDrawerItemCodebaseHost(SPLIT_SESSION_ID)).should("have.text", CODEBASE_HOST);
});

it("badges the codebase host only on the session that actually has one", () => {
  // Given a split session and an ordinary co-located one side by side
  mountRows([aSplitSession(), aCoLocatedSession()]);

  // Then only the split row carries the second badge. An unconditional badge would label every
  // pre-existing session as if its codebase were somewhere else.
  byTestId(sessionsDrawerItemCodebaseHost(SPLIT_SESSION_ID)).should("have.text", CODEBASE_HOST);
  byTestId(sessionsDrawerItemCodebaseHost(COLOCATED_SESSION_ID)).should("not.exist");

  // And both rows still name the daemon running their agent
  byTestId(sessionsDrawerItemHost(SPLIT_SESSION_ID)).should("have.text", AGENT_HOST);
  byTestId(sessionsDrawerItemHost(COLOCATED_SESSION_ID)).should("have.text", AGENT_HOST);
});
