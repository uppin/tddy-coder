import { describe, expect, it } from "bun:test";
import { aSessionEntry } from "../test-utils";
import { groupSessionsByStack } from "./sessionStackGroups";

/**
 * Tests for `groupSessionsByStack` — the util that partitions a session list into orchestrator
 * groups (parent + children) and flat (non-stack or orphan) entries.
 */

function aChildSession(sessionId: string, orchestratorSessionId: string, createdAt?: string) {
  return aSessionEntry({ sessionId, createdAt: createdAt ?? "2026-06-26T10:00:00Z", orchestratorSessionId });
}

describe("groupSessionsByStack", () => {
  it("returns empty groups and flat when session list is empty", () => {
    const result = groupSessionsByStack([]);
    expect(result.groups).toEqual([]);
    expect(result.flat).toEqual([]);
  });

  it("puts plain non-stack sessions into flat", () => {
    // Given — two sessions with no orchestratorSessionId
    const sessions = [
      aSessionEntry({ sessionId: "plain-1" }),
      aSessionEntry({ sessionId: "plain-2" }),
    ];

    // When
    const result = groupSessionsByStack(sessions);

    // Then — no groups, both in flat
    expect(result.groups).toHaveLength(0);
    expect(result.flat).toHaveLength(2);
    const flatIds = result.flat.map((s) => s.sessionId);
    expect(flatIds).toContain("plain-1");
    expect(flatIds).toContain("plain-2");
  });

  it("groups parent and children — parent not in flat, children not in flat", () => {
    // Given — one orchestrator and two children
    const orchestrator = aSessionEntry({ sessionId: "orch-1", createdAt: "2026-06-26T08:00:00Z" });
    const child1 = aChildSession("child-1", "orch-1", "2026-06-26T09:00:00Z");
    const child2 = aChildSession("child-2", "orch-1", "2026-06-26T10:00:00Z");
    const sessions = [orchestrator, child1, child2];

    // When
    const result = groupSessionsByStack(sessions);

    // Then
    expect(result.groups).toHaveLength(1);
    const group = result.groups[0]!;
    expect(group.parent.sessionId).toBe("orch-1");
    expect(group.children).toHaveLength(2);
    const childIds = group.children.map((c) => c.sessionId);
    expect(childIds).toContain("child-1");
    expect(childIds).toContain("child-2");

    // Neither the parent nor children appear in flat
    expect(result.flat).toHaveLength(0);
  });

  it("children within a group are sorted by createdAt ascending (oldest first)", () => {
    // Given — children created in reverse order
    const orch = aSessionEntry({ sessionId: "orch-sort", createdAt: "2026-06-26T06:00:00Z" });
    const newerChild = aChildSession("child-newer", "orch-sort", "2026-06-26T12:00:00Z");
    const olderChild = aChildSession("child-older", "orch-sort", "2026-06-26T07:00:00Z");
    const sessions = [orch, newerChild, olderChild];

    // When
    const result = groupSessionsByStack(sessions);

    // Then — older child first within the group
    const children = result.groups[0]!.children;
    expect(children[0]!.sessionId).toBe("child-older");
    expect(children[1]!.sessionId).toBe("child-newer");
  });

  it("puts a child with a missing orchestrator into flat (orphan child)", () => {
    // Given — child references an orchestrator that is not in the list
    const orphanChild = aChildSession("orphan-child", "missing-orch-99");
    const sessions = [orphanChild];

    // When
    const result = groupSessionsByStack(sessions);

    // Then — no groups formed; orphan child ends up in flat
    expect(result.groups).toHaveLength(0);
    expect(result.flat).toHaveLength(1);
    expect(result.flat[0]!.sessionId).toBe("orphan-child");
  });

  it("handles multiple independent orchestrator groups alongside flat sessions", () => {
    // Given — two separate stacks plus one plain session
    const orch1 = aSessionEntry({ sessionId: "orch-A", createdAt: "2026-06-26T08:00:00Z" });
    const child1 = aChildSession("child-of-A", "orch-A");
    const orch2 = aSessionEntry({ sessionId: "orch-B", createdAt: "2026-06-26T09:00:00Z" });
    const child2 = aChildSession("child-of-B", "orch-B");
    const plain = aSessionEntry({ sessionId: "plain-solo" });
    const sessions = [orch1, child1, orch2, child2, plain];

    // When
    const result = groupSessionsByStack(sessions);

    // Then — two groups, one flat entry
    expect(result.groups).toHaveLength(2);
    expect(result.flat).toHaveLength(1);
    expect(result.flat[0]!.sessionId).toBe("plain-solo");

    const groupParentIds = result.groups.map((g) => g.parent.sessionId);
    expect(groupParentIds).toContain("orch-A");
    expect(groupParentIds).toContain("orch-B");
  });

  it("does not include the orchestrator session in flat when it has children", () => {
    // Given — an orchestrator that is also a session in the list
    const orch = aSessionEntry({ sessionId: "orch-present" });
    const child = aChildSession("child-1", "orch-present");
    const sessions = [orch, child];

    // When
    const result = groupSessionsByStack(sessions);

    // Then — orchestrator must not appear in flat
    const flatIds = result.flat.map((s) => s.sessionId);
    expect(flatIds).not.toContain("orch-present");
    expect(result.groups).toHaveLength(1);
  });
});
