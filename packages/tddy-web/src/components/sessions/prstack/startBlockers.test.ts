import { describe, expect, it } from "bun:test";
import {
  resolveRepointTarget,
  startBlockers,
  type BranchRemoteState,
} from "./startBlockers";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `startBlockers.ts` — the two questions a planned-PR row asks about a node it has not
 * spawned yet: *why* can it not be started, and *where* would a repoint put it.
 *
 * `startBlockers` returns every reason, each carrying the text the row shows; an empty array means
 * startable. It replaces a boolean that could carry only one reason and no message.
 *
 * `resolveRepointTarget` is the web-side statement of the daemon's retain rule (D18): the first direct
 * parent that can serve as a base right now, else the project default branch. A target no parent owns
 * means every parent is dropped and the node's base collapses to the default.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Repointing a dead-end planned PR (D16–D18).
 */

const DEFAULT_BRANCH = "origin/master";
const PREDECESSOR_BRANCH = "feature/auth/token-store";
const SIBLING_BRANCH = "feature/auth/session-store";

/** A stack node with only the fields these tests care about; everything else gets a valid default. */
function aNode(overrides: Partial<StackNode> & { nodeId: string }): StackNode {
  return {
    nodeId: overrides.nodeId,
    title: overrides.title ?? overrides.nodeId,
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    childRecipe: "tdd",
    internalStatus: null,
    ...overrides,
  };
}

/** A branch whose `origin/<branch>` ref exists — a descendant can be based onto it. */
function anAvailableBranch(): BranchRemoteState {
  return { remote: { exists: true } };
}

/** A branch absent from `origin` — deleted after its PR merged, or never pushed. */
function aBranchAbsentFromOrigin(): BranchRemoteState {
  return { remote: { exists: false } };
}

describe("startBlockers", () => {
  it("reports no blockers for a root node", () => {
    // Given — no parents, so the base is the project default branch
    const n1 = aNode({ nodeId: "n1", title: "Add token store" });

    // When
    const blockers = startBlockers(n1, [n1], {});

    // Then — the default branch exists by construction
    expect(blockers).toEqual([]);
  });

  it("reports no blockers for a node that already owns a branch", () => {
    // Given — n2 owns a branch, and its predecessor's branch is gone from origin
    const n1 = aNode({ nodeId: "n1", title: "Add token store", branch: PREDECESSOR_BRANCH });
    const n2 = aNode({
      nodeId: "n2",
      title: "Add middleware",
      branch: "feature/auth/middleware",
      parents: ["n1"],
    });

    // When
    const blockers = startBlockers(n2, [n1, n2], {
      [PREDECESSOR_BRANCH]: aBranchAbsentFromOrigin(),
    });

    // Then — its spawn resumes the branch it owns, which creates nothing and fetches nothing
    expect(blockers).toEqual([]);
  });

  it("reports no blockers while the base branch resolution has not arrived", () => {
    // Given — n2's base is n1's branch, and no resolution has come back for it
    const n1 = aNode({ nodeId: "n1", title: "Add token store", branch: PREDECESSOR_BRANCH });
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n1"] });

    // When
    const blockers = startBlockers(n2, [n1, n2], {});

    // Then — an unanswered poll is unknown, never missing
    expect(blockers).toEqual([]);
  });

  it("names the base branch that is absent from origin", () => {
    // Given — n1's branch exists in the plan but not on origin
    const n1 = aNode({ nodeId: "n1", title: "Add token store", branch: PREDECESSOR_BRANCH });
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n1"] });

    // When
    const blockers = startBlockers(n2, [n1, n2], {
      [PREDECESSOR_BRANCH]: aBranchAbsentFromOrigin(),
    });

    // Then
    expect(blockers).toEqual([
      {
        kind: "base-branch-not-on-origin",
        branch: PREDECESSOR_BRANCH,
        message: `Base branch ${PREDECESSOR_BRANCH} is not on origin`,
      },
    ]);
  });

  it("names the parent that has not created its branch yet", () => {
    // Given — n1 holds only a planned branch name, so there is no ref for n2 to be based onto
    const n1 = aNode({
      nodeId: "n1",
      title: "Add token store",
      branchSuggestion: PREDECESSOR_BRANCH,
    });
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n1"] });

    // When
    const blockers = startBlockers(n2, [n1, n2], {});

    // Then
    expect(blockers).toEqual([
      {
        kind: "parent-has-no-branch",
        parentTitle: "Add token store",
        message: "Add token store has not created its branch yet",
      },
    ]);
  });

  it("reports that no predecessor owns a branch yet when the block is above a merged parent", () => {
    // Given — n1 (non-merged, branchless) → n2 (merged, branchless) → n3. n3's only direct parent has
    // merged, so the flat parent gate passes it; the chain above it still owns no ref at all, and a
    // merged parent's own `origin` ref may already be gone, so it is no base either.
    const n1 = aNode({ nodeId: "n1", title: "Add token store" });
    const n2 = aNode({
      nodeId: "n2",
      title: "Add session store",
      parents: ["n1"],
      prStatus: { phase: "merged" },
    });
    const n3 = aNode({ nodeId: "n3", title: "Add middleware", parents: ["n2"] });

    // When
    const blockers = startBlockers(n3, [n1, n2, n3], {});

    // Then
    expect(blockers).toEqual([
      { kind: "no-ancestor-branch", message: "No predecessor owns a branch yet" },
    ]);
  });

  it("ignores a parent id that matches no node in the stack", () => {
    // Given — a dangling parent reference, which is a malformed plan rather than an unmet dependency
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n-gone"] });

    // When
    const blockers = startBlockers(n2, [n2], {});

    // Then — the daemon's own gate likewise refuses only on a parent it can resolve
    expect(blockers).toEqual([]);
  });

  it("terminates on a parent cycle rather than recursing forever", () => {
    // Given — a malformed `stackPlanJson` whose parents form a cycle. This runs on every render, so a
    // bad plan must not be able to hang the screen.
    const n1 = aNode({ nodeId: "n1", title: "Add token store", parents: ["n2"] });
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n1"] });

    // When
    const blockers = startBlockers(n1, [n1, n2], {});

    // Then — n1's direct parent owns no branch, which is a plain unmet dependency. The base also
    // resolves to `no-ancestor-branch` here, but that is the *same fact* about the same node, so it is
    // not reported a second time (see "reports that no predecessor owns a branch yet ...", where the
    // block is above a merged parent and there is no blocking direct parent to name).
    expect(blockers).toEqual([
      {
        kind: "parent-has-no-branch",
        parentTitle: "Add middleware",
        message: "Add middleware has not created its branch yet",
      },
    ]);
  });

  it("reports the unmet parent ahead of a base branch that is absent from origin", () => {
    // Given — n3 depends on n1 (branch pushed but gone from origin) and n2 (planned only)
    const n1 = aNode({ nodeId: "n1", title: "Add token store", branch: PREDECESSOR_BRANCH });
    const n2 = aNode({
      nodeId: "n2",
      title: "Add session store",
      branchSuggestion: SIBLING_BRANCH,
    });
    const n3 = aNode({ nodeId: "n3", title: "Add middleware", parents: ["n1", "n2"] });

    // When
    const blockers = startBlockers(n3, [n1, n2, n3], {
      [PREDECESSOR_BRANCH]: aBranchAbsentFromOrigin(),
    });

    // Then — the unmet dependency comes first: it is the one with an action behind it
    expect(blockers).toEqual([
      {
        kind: "parent-has-no-branch",
        parentTitle: "Add session store",
        message: "Add session store has not created its branch yet",
      },
      {
        kind: "base-branch-not-on-origin",
        branch: PREDECESSOR_BRANCH,
        message: `Base branch ${PREDECESSOR_BRANCH} is not on origin`,
      },
    ]);
  });
});

describe("resolveRepointTarget", () => {
  it("resolves to the default branch when no parent can serve as a base", () => {
    // Given — the reported case: n1's PR merged and its branch was deleted, but the plan still says open
    const n1 = aNode({
      nodeId: "n1",
      title: "Add token store",
      branch: PREDECESSOR_BRANCH,
      prStatus: { phase: "open" },
    });
    const n2 = aNode({ nodeId: "n2", title: "Add middleware", parents: ["n1"] });

    // When
    const target = resolveRepointTarget(
      n2,
      [n1, n2],
      { [PREDECESSOR_BRANCH]: aBranchAbsentFromOrigin() },
      DEFAULT_BRANCH,
    );

    // Then
    expect(target).toBe(DEFAULT_BRANCH);
  });

  it("resolves to the surviving parent's branch when one of two parents is dead", () => {
    // Given — n1's branch is gone from origin, n2's is pushed and open
    const n1 = aNode({
      nodeId: "n1",
      title: "Add token store",
      branch: PREDECESSOR_BRANCH,
      prStatus: { phase: "open" },
    });
    const n2 = aNode({
      nodeId: "n2",
      title: "Add session store",
      branch: SIBLING_BRANCH,
      prStatus: { phase: "open" },
    });
    const n3 = aNode({ nodeId: "n3", title: "Add middleware", parents: ["n1", "n2"] });

    // When
    const target = resolveRepointTarget(
      n3,
      [n1, n2, n3],
      {
        [PREDECESSOR_BRANCH]: aBranchAbsentFromOrigin(),
        [SIBLING_BRANCH]: anAvailableBranch(),
      },
      DEFAULT_BRANCH,
    );

    // Then — the node stays stacked on the predecessor that is still usable
    expect(target).toBe(SIBLING_BRANCH);
  });

  it("resolves past a merged parent to the next usable one", () => {
    // Given — the original repoint case: n1 merged, n2 still open, both direct parents of n3
    const n1 = aNode({
      nodeId: "n1",
      title: "Add token store",
      branch: PREDECESSOR_BRANCH,
      prStatus: { phase: "merged" },
    });
    const n2 = aNode({
      nodeId: "n2",
      title: "Add session store",
      branch: SIBLING_BRANCH,
      prStatus: { phase: "open" },
    });
    const n3 = aNode({ nodeId: "n3", title: "Add middleware", parents: ["n1", "n2"] });

    // When
    const target = resolveRepointTarget(
      n3,
      [n1, n2, n3],
      {
        [PREDECESSOR_BRANCH]: anAvailableBranch(),
        [SIBLING_BRANCH]: anAvailableBranch(),
      },
      DEFAULT_BRANCH,
    );

    // Then — a merged parent is dropped even though its branch is still on origin
    expect(target).toBe(SIBLING_BRANCH);
  });

  it("resolves to the default branch for a root node", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", title: "Add token store" });

    // When
    const target = resolveRepointTarget(n1, [n1], {}, DEFAULT_BRANCH);

    // Then
    expect(target).toBe(DEFAULT_BRANCH);
  });
});
