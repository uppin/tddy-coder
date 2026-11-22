import { describe, expect, it } from "bun:test";
import { create, type MessageInitShape } from "@bufbuild/protobuf";
import { type SessionEntry, SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { stackParentCandidates } from "./stackParents";

/** Default active session; override only the fields that matter for a test. */
function aSessionEntry(overrides: MessageInitShape<typeof SessionEntrySchema> = {}): SessionEntry {
  return create(SessionEntrySchema, {
    sessionId: "session-default-1111-2222-3333-4444444444",
    createdAt: "2026-03-21T10:00:00Z",
    status: "active",
    repoPath: "/home/dev/project",
    pid: 12345,
    isActive: true,
    projectId: "proj-1",
    ...overrides,
  });
}

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
