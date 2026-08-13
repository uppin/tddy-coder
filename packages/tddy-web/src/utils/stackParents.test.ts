import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../test-utils";
import { stackBaseSessionCandidates, stackParentCandidates } from "./stackParents";

/**
 * Tests for `stackParentCandidates` — the function that identifies which sessions in a list act
 * as PR-stack orchestrators (i.e. are referenced as `orchestratorSessionId` by a child session).
 */

function aChildSession(sessionId: string, orchestratorSessionId: string) {
  return aSessionEntry({ sessionId, orchestratorSessionId });
}

describe("stackParentCandidates", () => {
  it("returns empty array when no sessions are orchestrators", () => {
    // Given — three plain sessions with no orchestratorSessionId
    const sessions = [
      aSessionEntry({ sessionId: "plain-1" }),
      aSessionEntry({ sessionId: "plain-2" }),
      aSessionEntry({ sessionId: "plain-3" }),
    ];

    // When
    const parents = stackParentCandidates(sessions);

    // Then — no session is referenced as a parent, so the result must be empty
    expect(parents).toEqual([]);
  });

  it("returns the orchestrator session when a child references it", () => {
    // Given — one orchestrator and one child that references it
    const orchestrator = aSessionEntry({ sessionId: "orch-1" });
    const child = aChildSession("child-1", "orch-1");
    const sessions = [orchestrator, child];

    // When
    const parents = stackParentCandidates(sessions);

    // Then — only the orchestrator session is returned as a candidate
    expect(parents).toHaveLength(1);
    expect(parents[0]!.sessionId).toBe("orch-1");
  });

  it("does not include the same orchestrator twice when multiple children reference it", () => {
    // Given — one orchestrator with two children
    const orchestrator = aSessionEntry({ sessionId: "orch-shared" });
    const child1 = aChildSession("child-a", "orch-shared");
    const child2 = aChildSession("child-b", "orch-shared");
    const sessions = [orchestrator, child1, child2];

    // When
    const parents = stackParentCandidates(sessions);

    // Then — deduplicated; only one entry for the orchestrator
    expect(parents).toHaveLength(1);
    expect(parents[0]!.sessionId).toBe("orch-shared");
  });

  it("returns empty array when the child's orchestrator is not in the list (orphan child)", () => {
    // Given — a child that references a missing orchestrator
    const orphanChild = aChildSession("child-orphan", "missing-orch-99");
    const sessions = [orphanChild];

    // When
    const parents = stackParentCandidates(sessions);

    // Then — no present parent found; result is empty
    expect(parents).toEqual([]);
  });

  it("returns empty array for an empty session list", () => {
    expect(stackParentCandidates([])).toEqual([]);
  });

  it("handles multiple independent orchestrators", () => {
    // Given — two independent orchestrators, each with a child
    const orch1 = aSessionEntry({ sessionId: "orch-A" });
    const orch2 = aSessionEntry({ sessionId: "orch-B" });
    const child1 = aChildSession("child-of-A", "orch-A");
    const child2 = aChildSession("child-of-B", "orch-B");
    const sessions = [orch1, orch2, child1, child2];

    // When
    const parents = stackParentCandidates(sessions);

    // Then — both orchestrators are candidates
    expect(parents).toHaveLength(2);
    const parentIds = parents.map((s) => s.sessionId).sort();
    expect(parentIds).toEqual(["orch-A", "orch-B"]);
  });
});

/**
 * Tests for `stackBaseSessionCandidates` — which sessions the new-session form may offer as the base
 * of a PR stack it is about to create.
 *
 * Owning a branch is not enough. Every descendant node's worktree is created off
 * `origin/<base branch>` in the orchestrator's own project on the orchestrator's own host, so a branch
 * from another repository or another host is one the stack cannot act on at all — and a branch another
 * orchestrator already tracks would end up with two owners holding repoint and pull authority over it.
 * Offering any of them means the operator picks a refusal (the daemon refuses all three before it
 * spawns), or worse — for a branch from another repository, this used to be accepted and failed much
 * later, as a git error on the first descendant's spawn.
 */
describe("stackBaseSessionCandidates", () => {
  const PROJECT = "proj-auth";
  const HOST = "host-a";
  const SCOPE = { projectId: PROJECT, daemonInstanceId: HOST };

  /** A session working on a branch in the scope's project on the scope's host. */
  function aSessionOnBranch(sessionId: string, overrides: Parameters<typeof aSessionEntry>[0] = {}) {
    return aSessionEntry({
      sessionId,
      branch: `feat/${sessionId}`,
      projectId: PROJECT,
      daemonInstanceId: HOST,
      ...overrides,
    });
  }

  function idsOf(sessions: ReturnType<typeof aSessionOnBranch>[]) {
    return stackBaseSessionCandidates(sessions, SCOPE).map((s) => s.sessionId);
  }

  it("offers a session that owns a branch in the same project on the same host", () => {
    // Given
    const sessions = [aSessionOnBranch("session-auth-store")];

    // When / Then
    expect(idsOf(sessions)).toEqual(["session-auth-store"]);
  });

  it("does not offer a session that owns no branch", () => {
    // Given a session that has not created its branch yet — there is no ref to base anything onto
    const sessions = [aSessionOnBranch("session-unstarted", { branch: "" })];

    // When / Then
    expect(idsOf(sessions)).toEqual([]);
  });

  it("does not offer a session from another project", () => {
    // Given a branch that lives in a different repository
    const sessions = [
      aSessionOnBranch("session-auth-store"),
      aSessionOnBranch("session-other-repo", { projectId: "proj-billing" }),
    ];

    // When / Then — a descendant based off `origin/feat/session-other-repo` would fail in this
    // project's repository, long after the orchestrator was created and looked seeded
    expect(idsOf(sessions)).toEqual(["session-auth-store"]);
  });

  it("does not offer a session from another host", () => {
    // Given a branch that exists in another daemon's checkout
    const sessions = [
      aSessionOnBranch("session-auth-store"),
      aSessionOnBranch("session-other-host", { daemonInstanceId: "host-b" }),
    ];

    // When / Then — the stack is operated on the host the orchestrator runs on
    expect(idsOf(sessions)).toEqual(["session-auth-store"]);
  });

  it("does not offer a session that is already a node of another orchestrator's stack", () => {
    // Given a session another orchestrator already tracks
    const sessions = [
      aSessionOnBranch("session-auth-store"),
      aSessionOnBranch("session-already-stacked", { orchestratorSessionId: "orchestrator-1" }),
    ];

    // When / Then — two orchestrators with repoint and pull authority over one branch is ambiguous
    // ownership
    expect(idsOf(sessions)).toEqual(["session-auth-store"]);
  });

  it("offers nothing while no project is resolved", () => {
    // Given a session whose own project is unknown as well — the only kind an unresolved scope could
    // match, and the reason the empty scope is refused outright rather than compared
    const sessions = [aSessionOnBranch("session-unknown-project", { projectId: "" })];

    // When
    const candidates = stackBaseSessionCandidates(sessions, {
      projectId: "",
      daemonInstanceId: HOST,
    });

    // Then nothing is offered: with no project there is no repository for a base to share, and
    // matching the empty string would offer every session whose project is also unknown
    expect(candidates).toEqual([]);
  });

  it("keeps the daemon's list order", () => {
    // Given three eligible sessions
    const sessions = [
      aSessionOnBranch("session-c"),
      aSessionOnBranch("session-a"),
      aSessionOnBranch("session-b"),
    ];

    // When / Then — the picker reads in the order the daemon reported, not a re-sorted one
    expect(idsOf(sessions)).toEqual(["session-c", "session-a", "session-b"]);
  });
});
