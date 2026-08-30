import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../../test-utils";
import { subagentSessionNodes, type SubagentSessionNode } from "./agentTree";

/**
 * The pure fold behind the Agents tab's tree: a flat `ListSessions` list plus the id of the session
 * the tab is about, folded into the subagent sessions beneath it.
 *
 * Every property here is a property of the *fold*, not of a rendering. A cycle driven through a
 * mounted tree either hangs the runner or says nothing this file does not already say, and a
 * grandchild's placement is decided here before any row exists.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § The Agents tab (AC53a, AC53e).
 */

const MAIN = "session-main-0000-0000-0000-000000000001";

/** A session spawned by `orchestrator` — the shape `spawn_conversation` leaves behind. */
function aSubagentOf(sessionId: string, orchestrator: string) {
  return aSessionEntry({ sessionId, orchestratorSessionId: orchestrator });
}

/** The session ids of a node list, in order — what "which rows, in which order" reduces to. */
function idsOf(nodes: ReadonlyArray<SubagentSessionNode>): string[] {
  return nodes.map((node) => node.session.sessionId);
}

describe("subagentSessionNodes", () => {
  it("returns no nodes when nothing names the main session as its orchestrator", () => {
    // Given a standalone session and a subagent of somebody else
    const sessions = [
      aSessionEntry({ sessionId: "standalone", orchestratorSessionId: "" }),
      aSubagentOf("elsewhere", "session-other-0000-0000-0000-000000000009"),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then
    expect(nodes).toEqual([]);
  });

  it("returns the sessions the main session spawned, in list order", () => {
    // Given two subagents of the main session, listed between sessions that are not its own
    const sessions = [
      aSubagentOf("cursor-child", MAIN),
      aSubagentOf("elsewhere", "session-other-0000-0000-0000-000000000009"),
      aSubagentOf("claude-child", MAIN),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — order follows the input, so two folds of one list agree
    expect(idsOf(nodes)).toEqual(["cursor-child", "claude-child"]);
  });

  it("nests a grandchild under its own parent rather than beside it", () => {
    // Given a subagent that spawned a subagent of its own
    const sessions = [aSubagentOf("child", MAIN), aSubagentOf("grandchild", "child")];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — the main session has exactly one child, and the grandchild hangs off it
    expect(idsOf(nodes)).toEqual(["child"]);
    expect(idsOf(nodes[0].children)).toEqual(["grandchild"]);
  });

  it("nests a third generation under the second", () => {
    // Given a three-deep spawn chain — the case a one-level grouping renders as three siblings
    const sessions = [
      aSubagentOf("child", MAIN),
      aSubagentOf("grandchild", "child"),
      aSubagentOf("great-grandchild", "grandchild"),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then
    expect(idsOf(nodes[0].children[0].children)).toEqual(["great-grandchild"]);
  });

  it("leaves the main session out of its own subagents when it names itself as orchestrator", () => {
    // Given a malformed self-reference beside a real subagent
    const sessions = [aSubagentOf(MAIN, MAIN), aSubagentOf("real-child", MAIN)];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then
    expect(idsOf(nodes)).toEqual(["real-child"]);
  });

  it("drops a subagent that names itself as its own orchestrator", () => {
    // Given a subagent of the main session that also claims to have spawned itself
    const sessions = [
      aSubagentOf("child", MAIN),
      aSessionEntry({ sessionId: "self-spawner", orchestratorSessionId: "self-spawner" }),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — it belongs to no branch of this tree, and it is not its own child either
    expect(idsOf(nodes)).toEqual(["child"]);
    expect(nodes[0].children).toEqual([]);
  });

  it("nests a chain three deep when nothing in it loops", () => {
    // Given an acyclic chain — the control for the cycle case below
    const sessions = [
      aSubagentOf("child", MAIN),
      aSubagentOf("grandchild", "child"),
      aSubagentOf("great-grandchild-2", "grandchild"),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then
    expect(idsOf(nodes[0].children[0].children)).toEqual(["great-grandchild-2"]);
  });

  it("stops at a session already on its own branch when the orchestrator links form a cycle", () => {
    // Given a cycle: "child" is a subagent of the main session, "grandchild" of "child", and a
    // second entry claims "child" was in turn spawned by "grandchild"
    const sessions = [
      aSubagentOf("child", MAIN),
      aSubagentOf("grandchild", "child"),
      aSessionEntry({ sessionId: "child", orchestratorSessionId: "grandchild" }),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — the branch ends rather than descending into "child" a second time
    expect(idsOf(nodes)).toEqual(["child"]);
    expect(idsOf(nodes[0].children)).toEqual(["grandchild"]);
    expect(nodes[0].children[0].children).toEqual([]);
  });

  it("leaves out a session whose orchestrator is not in the list", () => {
    // Given an orphan: its parent is on another host, or has been deleted
    const sessions = [
      aSubagentOf("child", MAIN),
      aSubagentOf("orphan", "session-vanished-0000-0000-0000-00000000000f"),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — an orphan promoted to the root would claim the main agent spawned it
    expect(idsOf(nodes)).toEqual(["child"]);
  });

  it("leaves out a session with no orchestrator at all", () => {
    // Given a standalone session sitting in the same list
    const sessions = [
      aSubagentOf("child", MAIN),
      aSessionEntry({ sessionId: "standalone", orchestratorSessionId: "" }),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then
    expect(idsOf(nodes)).toEqual(["child"]);
  });

  it("carries the whole session entry on each node, not just its id", () => {
    // Given a subagent with the fields a row renders
    const sessions = [
      aSessionEntry({
        sessionId: "child",
        orchestratorSessionId: MAIN,
        agent: "claude",
        model: "opus-4",
        sessionType: "claude-cli",
      }),
    ];

    // When
    const nodes = subagentSessionNodes(sessions, MAIN);

    // Then — a node that carried an id alone would force the row to re-scan the list to render
    expect(nodes[0].session.agent).toEqual("claude");
    expect(nodes[0].session.model).toEqual("opus-4");
    expect(nodes[0].session.sessionType).toEqual("claude-cli");
  });
});
