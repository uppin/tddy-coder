/**
 * Acceptance: the Agents tab is a **tree** — the session's main agent at the root, the roster agents
 * attached to it and the subagent sessions it spawned beneath it, and a subagent's own roster agents
 * and subagents beneath that.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § The Agents tab (AC50-AC53f).
 * Module notes: packages/tddy-web/docs/session-agent-tree.md.
 *
 * Two populations meet here, and the point of the tree is that they are shown as one hierarchy
 * rather than as two lists in two places:
 *
 *  - **Managed** roster agents, whose loop the facilitating daemon runs. They arrive on
 *    `StreamSessionAgents` and report `SessionAgentEntry.status` (#410).
 *  - **Non-managed** subagent *sessions* — claude-cli and cursor sessions spawned by the main agent.
 *    They arrive in the drawer's `ListSessions` list and report the **inferred**
 *    `SessionEntry.agent_status`, which the daemon derives by tailing that session's own
 *    conversation (#419, docs/ft/daemon/agent-session-status.md).
 *
 * Containment is asserted by scoping into a parent's *children* list. Two independent existence
 * checks would pass against the flat list this change replaces, which is exactly the regression
 * these cases have to catch.
 *
 * The pane is mounted rather than `SessionAgentTree` alone: the roster stream, the session list and
 * the detach flow all meet in the pane, and handing the tree a roster the spec made up would be the
 * fixture proving itself.
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import {
  SessionAgentStatus,
  SessionEntrySchema,
  type SessionEntry,
} from "../../src/gen/connection_pb";
import { SessionAgentRosterPane } from "../../src/components/sessions/SessionAgentRosterPane";
import {
  aSessionAgentRosterBackend,
  anActivity,
  anAgentDoing,
  anAttachedAgent,
  type RosterBackend,
} from "../support/rpc/sessionAgentRosterBackend";
import { sessionAgentRosterPage as page } from "../support/pages/sessionAgentRosterPage";
import { TEST_IDS, agentRosterRow, agentTreeSession } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HOST = "workstation-1";
const CODEBASE_HOST = "codebase-2";

const MAIN_SESSION_ID = "1780828020298-main";
const CURSOR_CHILD = "1780828020298-cursor-child";
const CLAUDE_CHILD = "1780828020298-claude-child";
const GRANDCHILD = "1780828020298-grandchild";
const SPLIT_CHILD = "1780828020298-split-child";
const SPLIT_CHILD_CODEBASE = "1780828020298-split-child-clone";

const EXPLORER = "explorer@workstation-1";
const REVIEWER = "reviewer@workstation-1";

/** A fixed "now", so an age in the rendered text is a fact about the fixture, not about the clock. */
const NOW_MS = 1_780_828_020_298;

function aSession(overrides: Partial<SessionEntry> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: MAIN_SESSION_ID,
    createdAt: "2026-08-30T09:00:00Z",
    status: "active",
    repoPath: "/home/dev/feature-alpha",
    pid: 90001,
    isActive: true,
    projectId: "proj-1",
    daemonInstanceId: HOST,
    sessionType: "claude-cli",
    agent: "claude",
    model: "opus-4",
    pendingElicitation: false,
    orchestratorSessionId: "",
    agentStatus: SessionAgentStatus.UNSPECIFIED,
    ...overrides,
  });
}

const MAIN_SESSION = aSession();

/** A session the main agent spawned — the shape `spawn_conversation` leaves behind. */
function aSubagentSession(
  sessionId: string,
  orchestrator: string,
  overrides: Partial<SessionEntry> = {},
): SessionEntry {
  return aSession({ sessionId, orchestratorSessionId: orchestrator, ...overrides });
}

/**
 * Mount the Agents tab for `MAIN_SESSION`, with `sessions` standing in for the drawer's list.
 *
 * `onSwitchSubagent` is a stub so a case can say *which* session a Switch reported — a count alone
 * would pass for the wrong row.
 */
function mountTree(roster: RosterBackend, sessions: SessionEntry[]) {
  cy.mountWithRpc(
    <SessionAgentRosterPane
      session={MAIN_SESSION}
      sessions={sessions}
      sessionToken="tok"
      daemonConnected
      onSwitchSubagent={cy.stub().as("switchSubagent")}
    />,
    roster.backend,
  );
}

/** The session ids `StreamSessionAgents` was asked about, in call order. */
function rosterReadSessionIds(roster: RosterBackend): string[] {
  return roster.rosterReadsAddressed().map((read) => read.sessionId);
}

// ---------------------------------------------------------------------------

describe("Agents tab — subagents as children of the main agent", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
    // The relative age in a last-activity line is rendered against the wall clock; pinning it keeps
    // "just now" from becoming "1s ago" on a slow machine.
    cy.clock(NOW_MS);
  });

  // -------------------------------------------------------------------------
  // AC1-AC5, AC10 — the shape of the tree
  // -------------------------------------------------------------------------

  it("renders the session's own main agent as the root of the tree", () => {
    // Given a session with nothing attached and nothing spawned
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    mountTree(roster, [MAIN_SESSION]);

    // Then — the root is the agent everything else hangs off, named by what the session runs
    page.rootRow().should("contain.text", "claude").and("contain.text", "opus-4");
  });

  it("renders an attached roster agent as a child of the main agent", () => {
    // Given one managed agent on the roster
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });

    // When
    mountTree(roster, [MAIN_SESSION]);

    // Then — inside the main agent's children, not merely somewhere in the pane
    page.assertRosterAgentUnderMainAgent(EXPLORER);
  });

  it("renders a session the main agent spawned as a child of the main agent", () => {
    // Given a cursor session spawned by this session
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    // Then
    page.assertSubagentUnderMainAgent(CURSOR_CHILD);
  });

  it("nests a subagent's own subagent under it rather than under the main agent", () => {
    // Given a two-deep spawn chain — the case a flat list renders as two siblings
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID),
      aSubagentSession(GRANDCHILD, CURSOR_CHILD),
    ]);

    // When the operator opens the subagent that spawned it
    page.expandSubagent(CURSOR_CHILD);

    // Then
    page.assertSubagentUnderSubagent(CURSOR_CHILD, GRANDCHILD);
  });

  it("renders a subagent session's own roster agents under it once expanded", () => {
    // Given the main agent holds one roster agent and its subagent holds another
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
      rostersBySession: { [CURSOR_CHILD]: [anAttachedAgent(REVIEWER)] },
    });
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    // When
    page.expandSubagent(CURSOR_CHILD);

    // Then — each roster agent sits under the agent that attached it
    page.assertRosterAgentUnderSubagent(CURSOR_CHILD, REVIEWER);
    page.assertRosterAgentUnderMainAgent(EXPLORER);
  });

  it("records how deep each row sits", () => {
    // Depth in the DOM rather than in a margin: a nested row that lost its parent still indents.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID),
      aSubagentSession(GRANDCHILD, CURSOR_CHILD),
    ]);
    page.expandSubagent(CURSOR_CHILD);

    page.assertRowDepth(TEST_IDS.agentTreeRoot, 0);
    page.assertRowDepth(agentRosterRow(EXPLORER), 1);
    page.assertRowDepth(agentTreeSession(CURSOR_CHILD), 1);
    page.assertRowDepth(agentTreeSession(GRANDCHILD), 2);
  });

  it("leaves a session spawned by somebody else out of the tree", () => {
    // Given a session of another orchestrator sitting in the same drawer list
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID),
      aSubagentSession(CLAUDE_CHILD, "1780828020298-somebody-else"),
    ]);

    // Then — a row for it would claim this main agent spawned it
    page.subagentRow(CURSOR_CHILD).should("exist");
    page.subagentRow(CLAUDE_CHILD).should("not.exist");
  });

  // -------------------------------------------------------------------------
  // AC11-AC16 — one badge for both kinds of row
  // -------------------------------------------------------------------------

  it("shows what a non-managed subagent session is doing, inferred from its own conversation", () => {
    // Given a claude-cli subagent the daemon has observed inside a tool call
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID, {
        agentStatus: SessionAgentStatus.EXECUTING_TOOL,
        lastActivity: anActivity("Bash: cargo test", NOW_MS - 12_000),
      }),
    ]);

    // Then — the same token a managed agent's badge carries, because it is the same enum
    page.assertSubagentStatus(CLAUDE_CHILD, "executing-tool");
    page.subagentStatus(CLAUDE_CHILD).should("have.text", "executing tool");
  });

  it("shows a subagent session the daemon has nothing to say about as unknown, not as idle", () => {
    // Given a `tool` subagent — a session type the daemon does not tail at all
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID, {
        sessionType: "tool",
        agentStatus: SessionAgentStatus.UNSPECIFIED,
      }),
    ]);

    // Then — "idle" would read as "free, ready for work", which is a claim nobody has made
    page.assertSubagentStatus(CURSOR_CHILD, "unknown");
    page.subagentStatus(CURSOR_CHILD).should("have.text", "unknown");
  });

  it("shows what a subagent session was last seen doing, and how long ago", () => {
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID, {
        agentStatus: SessionAgentStatus.IDLE,
        lastActivity: anActivity("answered (412 chars)", NOW_MS - 4 * 60_000),
      }),
    ]);

    page
      .subagentLastActivity(CLAUDE_CHILD)
      .should("contain.text", "answered (412 chars)")
      .and("contain.text", "4m ago");
  });

  it("shows no last-activity line for a subagent session nothing has been observed of", () => {
    // An empty line reserved for a session with no history is a row that looks like it lost one.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    page.subagentRow(CURSOR_CHILD).should("exist");
    page.subagentLastActivity(CURSOR_CHILD).should("not.exist");
  });

  it("ages a subagent session's last-activity line without a new session list", () => {
    // Given a subagent last seen just now
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID, {
        agentStatus: SessionAgentStatus.IDLE,
        lastActivity: anActivity("answered", NOW_MS),
      }),
    ]);
    page.subagentLastActivity(CLAUDE_CHILD).should("contain.text", "just now");

    // When time passes and no new list arrives — an idle subagent produces no updates at all
    cy.tick(5 * 60_000);

    // Then
    page.subagentLastActivity(CLAUDE_CHILD).should("contain.text", "5m ago");
  });

  it("shows what the main agent itself is doing at the root", () => {
    // Given the session's own inferred status
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    // When
    cy.mountWithRpc(
      <SessionAgentRosterPane
        session={aSession({
          agentStatus: SessionAgentStatus.WAITING_FOR_INPUT,
          lastActivity: anActivity("needs approval to run: rm -rf build/", NOW_MS),
        })}
        sessions={[MAIN_SESSION]}
        sessionToken="tok"
        daemonConnected
        onSwitchSubagent={cy.stub().as("switchSubagent")}
      />,
      roster.backend,
    );

    // Then
    page.assertMainAgentStatus("waiting-for-input");
    page.rootLastActivity().should("contain.text", "needs approval to run: rm -rf build/");
  });

  it("says which rows are agents this daemon manages and which are sessions of their own", () => {
    // Given one of each under the main agent
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });

    // When
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    // Then — a label alone cannot say which of the two a row is, and they afford different actions
    page.assertRowKind(TEST_IDS.agentTreeRoot, "main");
    page.assertRowKind(agentRosterRow(EXPLORER), "roster");
    page.assertRowKind(agentTreeSession(CURSOR_CHILD), "session");
  });

  // -------------------------------------------------------------------------
  // AC17-AC19 — what an operator can do to each kind of row
  // -------------------------------------------------------------------------

  it("focuses a subagent session's runtime when its Switch is clicked", () => {
    // Given two subagents, so a switch that reported the wrong one would be visible
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID),
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID),
    ]);

    // When
    page.clickSwitch(CLAUDE_CHILD);

    // Then
    cy.get("@switchSubagent").should("have.been.calledOnceWith", CLAUDE_CHILD);
  });

  it("offers no detach on a subagent session row", () => {
    // There is no roster entry behind a subagent session — a Detach would have nothing to send.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    page.subagentSwitchBtn(CURSOR_CHILD).should("exist");
    page.assertNoDetachOnSubagent(CURSOR_CHILD);
  });

  it("keeps detach on a roster agent row", () => {
    // Given a managed agent beside a subagent session
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 1,
      initial: [anAttachedAgent(EXPLORER)],
    });
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);
    page.assertRosterAgentUnderMainAgent(EXPLORER);

    // When
    page.clickDetach(EXPLORER);

    // Then — a local agent's detach asks nothing and goes straight out, from inside the tree
    page.detachConfirmation().should("not.exist");
    cy.wrap(null).should(() => {
      expect(roster.detachedAgentIds()).to.deep.equal([EXPLORER]);
    });
  });

  // -------------------------------------------------------------------------
  // AC20-AC22 — what a collapsed node costs, and where an expanded one reads
  // -------------------------------------------------------------------------

  it("opens no roster stream for a collapsed subagent session", () => {
    // Given a subagent nobody has expanded. The Agents tab stays open for the life of the
    // inspector, so a stream per descendant would be held for that whole time.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
      rostersBySession: { [CURSOR_CHILD]: [anAttachedAgent(REVIEWER)] },
    });

    // When
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);
    page.subagentRow(CURSOR_CHILD).should("exist");

    // Then — the root's roster was read and the subagent's was not
    cy.wrap(null).should(() => {
      expect(rosterReadSessionIds(roster)).to.deep.equal([MAIN_SESSION_ID]);
    });
  });

  it("reads a subagent session's roster once it is expanded", () => {
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
      rostersBySession: { [CURSOR_CHILD]: [anAttachedAgent(REVIEWER)] },
    });
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    // When
    page.expandSubagent(CURSOR_CHILD);

    // Then
    page.assertRosterAgentUnderSubagent(CURSOR_CHILD, REVIEWER);
    cy.wrap(null).should(() => {
      expect(rosterReadSessionIds(roster)).to.deep.equal([MAIN_SESSION_ID, CURSOR_CHILD]);
    });
  });

  it("reads an expanded subagent's roster on the host that holds its codebase", () => {
    // Given a subagent that is itself split: its agent runs here, its worktree and its roster live
    // on another host. Reading the agent half would return an empty list beside the real one.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
      rostersBySession: { [SPLIT_CHILD_CODEBASE]: [anAttachedAgent(REVIEWER)] },
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(SPLIT_CHILD, MAIN_SESSION_ID, {
        codebaseSessionId: SPLIT_CHILD_CODEBASE,
        codebaseDaemonInstanceId: CODEBASE_HOST,
      }),
    ]);

    // When
    page.expandSubagent(SPLIT_CHILD);

    // Then — the read names the codebase half on the codebase host
    cy.wrap(null).should(() => {
      expect(roster.rosterReadsAddressed()).to.deep.equal([
        { sessionId: MAIN_SESSION_ID, daemonInstanceId: HOST },
        { sessionId: SPLIT_CHILD_CODEBASE, daemonInstanceId: CODEBASE_HOST },
      ]);
    });
  });

  it("names the host when an expanded subagent's roster cannot be read", () => {
    // Given a subagent whose codebase host is unreachable. Rendering an empty child list would say
    // "this subagent has nobody working for it", which is a claim no daemon made — the same reason
    // the pane refuses to show an unreadable root roster as an empty one.
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
      rosterFailuresBySession: { [CURSOR_CHILD]: "codebase-2 is not reachable" },
    });
    mountTree(roster, [MAIN_SESSION, aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID)]);

    // When
    page.expandSubagent(CURSOR_CHILD);

    // Then
    page.subagentRosterError(CURSOR_CHILD).should("have.text", "codebase-2 is not reachable");
  });

  it("keeps one subagent's unreadable roster off its sibling's row", () => {
    // Given two subagents, only one of whose hosts is unreachable
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
      rostersBySession: { [CLAUDE_CHILD]: [anAttachedAgent(REVIEWER)] },
      rosterFailuresBySession: { [CURSOR_CHILD]: "codebase-2 is not reachable" },
    });
    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CURSOR_CHILD, MAIN_SESSION_ID),
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID),
    ]);

    // When both are opened
    page.expandSubagent(CURSOR_CHILD);
    page.expandSubagent(CLAUDE_CHILD);

    // Then the healthy one shows its roster and says nothing about an error
    page.assertRosterAgentUnderSubagent(CLAUDE_CHILD, REVIEWER);
    page.subagentRosterError(CLAUDE_CHILD).should("not.exist");
    page.subagentRosterError(CURSOR_CHILD).should("exist");
  });

  // -------------------------------------------------------------------------
  // The pane's existing states survive the tree
  // -------------------------------------------------------------------------

  it("shows the empty state when the session has neither roster agents nor subagents", () => {
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    mountTree(roster, [MAIN_SESSION]);

    page.empty().should("exist");
  });

  it("shows the tree when the session has subagents but no roster agents", () => {
    // The empty state speaks for the roster alone; a spawned subagent is not "no agents attached".
    const roster = aSessionAgentRosterBackend({
      sessionId: MAIN_SESSION_ID,
      rev: 0,
      initial: [],
    });

    mountTree(roster, [
      MAIN_SESSION,
      aSubagentSession(CLAUDE_CHILD, MAIN_SESSION_ID, {
        agentStatus: SessionAgentStatus.RUNNING,
      }),
    ]);

    page.empty().should("not.exist");
    page.assertSubagentUnderMainAgent(CLAUDE_CHILD);
  });
});
