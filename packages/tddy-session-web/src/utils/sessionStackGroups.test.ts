import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../../test-utils/builders";
import type { SubagentSessionNode } from "../agentTree";
import { groupSessionsByStack, partitionSessionsByActivity } from "./sessionStackGroups";

/**
 * Tests for `groupSessionsByStack` — the util that folds a session list into the drawer's rows: each
 * session that no other listed session started is a top-level row, and a session started by another
 * (`orchestratorSessionId`) sits beneath its starter — and for `partitionSessionsByActivity`, which
 * splits the list into active / remaining and orders each side.
 *
 * PRD: docs/ft/web/session-drawer.md § Row order
 */

/** The rows in drawer order, each as `<sessionId>@<depth>`. */
function outline(rows: ReadonlyArray<SubagentSessionNode>, depth = 0): string[] {
  return rows.flatMap((row) => [`${row.session.sessionId}@${depth}`, ...outline(row.children, depth + 1)]);
}

function aStartedSession(sessionId: string, createdAt: string, orchestratorSessionId = "") {
  return aSessionEntry({ sessionId, createdAt, orchestratorSessionId });
}

/** An inactive session last updated at `updatedAt`, with its agent last seen at `lastActivityMs`. */
function anEndedSession(
  sessionId: string,
  { updatedAt, lastActivityMs, orchestratorSessionId = "" }: { updatedAt: string; lastActivityMs?: number; orchestratorSessionId?: string },
) {
  return aSessionEntry({
    sessionId,
    isActive: false,
    status: "exited",
    pid: 0,
    createdAt: "2026-11-01T00:00:00Z",
    updatedAt,
    orchestratorSessionId,
    lastActivity: lastActivityMs === undefined ? undefined : { atUnixMs: BigInt(lastActivityMs), summary: "did a thing" },
  });
}

describe("groupSessionsByStack", () => {
  it("returns no rows for an empty session list", () => {
    expect(groupSessionsByStack([]).rows).toEqual([]);
  });

  it("lists sessions nobody started newest-first", () => {
    // Given — three plain sessions, listed out of order
    const sessions = [
      aStartedSession("middle", "2026-11-20T10:00:00Z"),
      aStartedSession("oldest", "2026-11-19T10:00:00Z"),
      aStartedSession("newest", "2026-11-21T10:00:00Z"),
    ];

    // When
    const { rows } = groupSessionsByStack(sessions);

    // Then
    expect(outline(rows)).toEqual(["newest@0", "middle@0", "oldest@0"]);
  });

  it("places a started session right after its starter, its children newest-first", () => {
    // Given — an orchestrator with two children, and plain sessions started before and after it
    const sessions = [
      aStartedSession("older-plain", "2026-11-20T08:00:00Z"),
      aStartedSession("child-first", "2026-11-20T10:00:00Z", "orch"),
      aStartedSession("orch", "2026-11-20T09:00:00Z"),
      aStartedSession("newer-plain", "2026-11-20T12:00:00Z"),
      aStartedSession("child-second", "2026-11-20T11:00:00Z", "orch"),
    ];

    // When
    const { rows } = groupSessionsByStack(sessions);

    // Then — the orchestrator keeps its own place among the top-level rows; its children follow it
    expect(outline(rows)).toEqual([
      "newer-plain@0",
      "orch@0",
      "child-second@1",
      "child-first@1",
      "older-plain@0",
    ]);
  });

  it("nests a session started by a child under that child", () => {
    // Given — orch started worker, and worker started helper
    const sessions = [
      aStartedSession("helper", "2026-11-20T11:00:00Z", "worker"),
      aStartedSession("worker", "2026-11-20T10:00:00Z", "orch"),
      aStartedSession("orch", "2026-11-20T09:00:00Z"),
    ];

    // When
    const { rows } = groupSessionsByStack(sessions);

    // Then — each session appears once, under its own starter
    expect(outline(rows)).toEqual(["orch@0", "worker@1", "helper@2"]);
  });

  it("lists a session whose starter is not in the list as a top-level row", () => {
    // Given — a child whose orchestrator is not listed
    const sessions = [aStartedSession("orphan", "2026-11-20T10:00:00Z", "missing-orch")];

    // When
    const { rows } = groupSessionsByStack(sessions);

    // Then
    expect(outline(rows)).toEqual(["orphan@0"]);
  });

  it("keeps every session of an orchestrator cycle, each exactly once", () => {
    // Given — two sessions naming each other as their orchestrator (the list merges several hosts'
    // answers, so a cycle is a shape the drawer has to survive), plus one naming itself
    const sessions = [
      aStartedSession("ping", "2026-11-20T10:00:00Z", "pong"),
      aStartedSession("pong", "2026-11-20T09:00:00Z", "ping"),
      aStartedSession("narcissus", "2026-11-20T08:00:00Z", "narcissus"),
    ];

    // When
    const ids = outline(groupSessionsByStack(sessions).rows).map((row) => row.split("@")[0]);

    // Then
    expect([...ids].sort()).toEqual(["narcissus", "ping", "pong"]);
  });
});

/**
 * `partitionSessionsByActivity` splits a session list into an *active* partition (green/yellow dots:
 * `connectionStatusForSession !== "disconnected"`, i.e. `isActive`, whether or not it is also
 * eliciting) and a *remaining* partition (grey/disconnected), each independently stack-folded and
 * ordered, with per-partition counts.
 */
describe("partitionSessionsByActivity", () => {
  it("returns empty active and remaining partitions for an empty list", () => {
    const result = partitionSessionsByActivity([]);

    expect(result.activeCount).toBe(0);
    expect(result.remainingCount).toBe(0);
    expect(result.active.rows).toEqual([]);
    expect(result.remaining.rows).toEqual([]);
  });

  it("routes a connected (green) session to active and a disconnected (grey) session to remaining", () => {
    // Given — one green session and one grey session
    const green = aSessionEntry({ sessionId: "green-1", isActive: true, pendingElicitation: false });
    const grey = aSessionEntry({ sessionId: "grey-1", isActive: false, pendingElicitation: false });

    // When
    const result = partitionSessionsByActivity([green, grey]);

    // Then
    expect(outline(result.active.rows)).toEqual(["green-1@0"]);
    expect(outline(result.remaining.rows)).toEqual(["grey-1@0"]);
    expect(result.activeCount).toBe(1);
    expect(result.remainingCount).toBe(1);
  });

  it("routes a needs-input (yellow) session to the active partition", () => {
    // Given — a live session waiting for human input (yellow dot). Elicitation marks a session
    // needs-input only while it is actually running; a dead one carrying the flag is grey.
    const yellow = aSessionEntry({ sessionId: "yellow-1", isActive: true, pendingElicitation: true });

    // When
    const result = partitionSessionsByActivity([yellow]);

    // Then — the yellow session counts as active, not remaining
    expect(outline(result.active.rows)).toEqual(["yellow-1@0"]);
    expect(result.activeCount).toBe(1);
    expect(result.remaining.rows).toEqual([]);
    expect(result.remainingCount).toBe(0);
  });

  it("counts each partition by the number of sessions it contains", () => {
    // Given — two green, one yellow, one grey
    const sessions = [
      aSessionEntry({ sessionId: "g1", isActive: true }),
      aSessionEntry({ sessionId: "g2", isActive: true }),
      aSessionEntry({ sessionId: "y1", isActive: true, pendingElicitation: true }),
      aSessionEntry({ sessionId: "d1", isActive: false, pendingElicitation: false }),
    ];

    // When
    const result = partitionSessionsByActivity(sessions);

    // Then — three active (2 green + 1 yellow), one remaining
    expect(result.activeCount).toBe(3);
    expect(result.remainingCount).toBe(1);
  });

  it("orders the active partition newest-started first, each child right after its starter", () => {
    // Given — live sessions, one stack among them
    const sessions = [
      aSessionEntry({ sessionId: "old", isActive: true, createdAt: "2026-11-20T08:00:00Z" }),
      aSessionEntry({ sessionId: "orch", isActive: true, createdAt: "2026-11-20T09:00:00Z" }),
      aSessionEntry({ sessionId: "kid-1", isActive: true, createdAt: "2026-11-20T10:00:00Z", orchestratorSessionId: "orch" }),
      aSessionEntry({ sessionId: "kid-2", isActive: true, createdAt: "2026-11-20T11:00:00Z", orchestratorSessionId: "orch" }),
      aSessionEntry({ sessionId: "new", isActive: true, createdAt: "2026-11-20T12:00:00Z" }),
    ];

    // When
    const result = partitionSessionsByActivity(sessions);

    // Then
    expect(outline(result.active.rows)).toEqual(["new@0", "orch@0", "kid-2@1", "kid-1@1", "old@0"]);
  });

  it("orders the remaining partition by the later of last activity and last update, most recent first", () => {
    // Given — ended sessions whose latest moment is, in turn, an update or an agent activity
    const sessions = [
      // Updated (e.g. stopped) most recently, though its agent went quiet long before
      anEndedSession("stopped-last", { updatedAt: "2026-11-24T12:00:00Z", lastActivityMs: Date.parse("2026-11-20T00:00:00Z") }),
      // Agent active more recently than anything else here except stopped-last's update
      anEndedSession("active-late", { updatedAt: "2026-11-21T00:00:00Z", lastActivityMs: Date.parse("2026-11-24T11:00:00Z") }),
      // No observed activity at all — its update time alone places it
      anEndedSession("quiet", { updatedAt: "2026-11-23T00:00:00Z" }),
      anEndedSession("ancient", { updatedAt: "2026-11-01T00:00:00Z", lastActivityMs: Date.parse("2026-11-02T00:00:00Z") }),
    ];

    // When
    const result = partitionSessionsByActivity(sessions);

    // Then
    expect(outline(result.remaining.rows)).toEqual(["stopped-last@0", "active-late@0", "quiet@0", "ancient@0"]);
  });

  it("orders an ended stack's children by the same recency, right after their starter", () => {
    // Given — an ended orchestrator and two ended children, the earlier-started one stopping last
    const sessions = [
      anEndedSession("orch", { updatedAt: "2026-11-22T00:00:00Z" }),
      anEndedSession("kid-stopped-first", { updatedAt: "2026-11-21T00:00:00Z", orchestratorSessionId: "orch" }),
      anEndedSession("kid-stopped-last", { updatedAt: "2026-11-23T00:00:00Z", orchestratorSessionId: "orch" }),
      anEndedSession("plain", { updatedAt: "2026-11-24T00:00:00Z" }),
    ];

    // When
    const result = partitionSessionsByActivity(sessions);

    // Then
    expect(outline(result.remaining.rows)).toEqual([
      "plain@0",
      "orch@0",
      "kid-stopped-last@1",
      "kid-stopped-first@1",
    ]);
  });

  it("splits a mixed-activity stack across partitions by each session's own status", () => {
    // Given — an active orchestrator with a disconnected child
    const orch = aSessionEntry({ sessionId: "orch-m", isActive: true });
    const child = aSessionEntry({
      sessionId: "child-m",
      isActive: false,
      orchestratorSessionId: "orch-m",
    });

    // When
    const result = partitionSessionsByActivity([orch, child]);

    // Then — the parent is a childless row in active (its only child left the partition); the child
    // is a top-level row in remaining (its parent is absent there).
    expect(outline(result.active.rows)).toEqual(["orch-m@0"]);
    expect(outline(result.remaining.rows)).toEqual(["child-m@0"]);
  });
});
