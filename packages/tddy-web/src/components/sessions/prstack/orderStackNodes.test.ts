import { describe, expect, it } from "bun:test";
import { aStackNode } from "../../../test-utils";
import { orderStackNodes } from "./orderStackNodes";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `orderStackNodes` — the order the planned-PR list renders its rows in.
 *
 * Order used to be derived entirely from the dependency graph, which meant a merge, a repoint or a
 * re-parenting silently rewrote the operator's view: a row they were reading jumped position as a
 * consequence of an unrelated event. The reading order is now a persisted per-node position, and the
 * dependency graph is free to change underneath it without moving anything.
 *
 * A plan authored before display order existed carries none. Such a plan falls back to topological
 * order **wholesale** rather than per node: a half-numbered plan has no coherent total order, and
 * interleaving real positions with invented ones can render a child above its parent — a worse lie
 * than one render of a correct derived order. The next write to the stack numbers every node, after
 * which the fallback stops applying.
 */

/**
 * A node assembled without `parseStackPlan` — which is the only producer that always writes
 * `displayOrder`. Such a node carries no position field at all, which is a different value from
 * `null` and means the same thing.
 */
function aNodeCarryingNoDisplayOrderField(
  overrides: Partial<StackNode> & { nodeId: string },
): StackNode {
  const node = aStackNode(overrides);
  delete (node as Partial<StackNode>).displayOrder;
  return node;
}

const idsOf = (nodes: StackNode[]) => nodes.map((n) => n.nodeId);

describe("orderStackNodes", () => {
  describe("when every node carries a persisted position", () => {
    it("renders the rows in the order the plan persists", () => {
      // Given
      const nodes = [
        aStackNode({ nodeId: "n1", displayOrder: 2 }),
        aStackNode({ nodeId: "n2", displayOrder: 0 }),
        aStackNode({ nodeId: "n3", displayOrder: 1 }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n2", "n3", "n1"]);
    });

    it("renders a node above its parent when the persisted position places it there", () => {
      // Given — the reading order and the dependency graph are allowed to disagree
      const nodes = [
        aStackNode({ nodeId: "n1", displayOrder: 1 }),
        aStackNode({ nodeId: "n2", parents: ["n1"], displayOrder: 0 }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n2", "n1"]);
    });

    it("keeps a row in its position when its parents change under it", () => {
      // Given — the same nodes, re-parented as a repoint would leave them
      const before = [
        aStackNode({ nodeId: "n1", displayOrder: 0 }),
        aStackNode({ nodeId: "n2", parents: ["n1"], displayOrder: 1 }),
        aStackNode({ nodeId: "n3", parents: ["n2"], displayOrder: 2 }),
      ];
      const afterRepoint = [
        aStackNode({ nodeId: "n1", displayOrder: 0 }),
        aStackNode({ nodeId: "n2", parents: ["n1"], displayOrder: 1 }),
        aStackNode({ nodeId: "n3", parents: [], displayOrder: 2 }),
      ];

      // When
      const orderedBefore = orderStackNodes(before);
      const orderedAfter = orderStackNodes(afterRepoint);

      // Then — this is the whole point: the DAG moved and the list did not
      expect(idsOf(orderedAfter)).toEqual(idsOf(orderedBefore));
    });

    it("keeps a row in its position when a predecessor merges under it", () => {
      // Given
      const nodes = [
        aStackNode({ nodeId: "n1", displayOrder: 0, prStatus: { phase: "merged" } }),
        aStackNode({ nodeId: "n2", parents: ["n1"], displayOrder: 1 }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });

    it("keeps nodes that share a position in the order the plan lists them", () => {
      // Given
      const nodes = [
        aStackNode({ nodeId: "n1", displayOrder: 0 }),
        aStackNode({ nodeId: "n2", displayOrder: 0 }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });
  });

  describe("when the plan carries no persisted positions", () => {
    it("falls back to topological order, roots before their dependents", () => {
      // Given
      const nodes = [aStackNode({ nodeId: "n2", parents: ["n1"] }), aStackNode({ nodeId: "n1" })];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });

    it("renders a node whose parent does not exist last rather than dropping it", () => {
      // Given — a plan referencing a node that is not there is malformed, not a reason to hide a row
      const nodes = [aStackNode({ nodeId: "n2", parents: ["gone"] }), aStackNode({ nodeId: "n1" })];

      // When
      const ordered = orderStackNodes(nodes);

      // Then — the placeable nodes go first; the unresolvable one trails them in plan order
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });
  });

  describe("when a node carries no position field at all", () => {
    it("falls back to topological order rather than sorting on a missing position", () => {
      // Given — an absent field sorts as `NaN`, which leaves the plan's raw order untouched
      const nodes = [
        aNodeCarryingNoDisplayOrderField({ nodeId: "n2", parents: ["n1"] }),
        aNodeCarryingNoDisplayOrderField({ nodeId: "n1" }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });
  });

  describe("when only some nodes carry a persisted position", () => {
    it("falls back to topological order rather than interleaving invented positions", () => {
      // Given — numbering only n2 would otherwise place it above the parent it depends on
      const nodes = [
        aStackNode({ nodeId: "n2", parents: ["n1"], displayOrder: 0 }),
        aStackNode({ nodeId: "n1" }),
      ];

      // When
      const ordered = orderStackNodes(nodes);

      // Then
      expect(idsOf(ordered)).toEqual(["n1", "n2"]);
    });
  });

  it("returns nothing for a plan with no nodes", () => {
    // Given / When / Then
    expect(orderStackNodes([])).toEqual([]);
  });
});
