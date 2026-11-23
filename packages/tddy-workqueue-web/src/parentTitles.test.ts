import { describe, expect, it } from "bun:test";
import { aStackNode } from "../test-utils/builders";
import { parentTitles } from "./parentTitles";

/**
 * Tests for `parentTitles` — naming the planned PRs a row is stacked on.
 *
 * `WorkItem.parents` records node ids ("n1", "n4"), which say nothing to an operator reading the
 * panel. A dangling id is skipped rather than rendered: a plan referencing a node that does not
 * exist is malformed, not an unmet dependency, and this is the stance `deriveStackBaseBranch` and
 * `startBlockers` already take.
 */

const A_STACK = [
  aStackNode({ itemId: "n1", title: "Add token store" }),
  aStackNode({ itemId: "n2", title: "Add auth middleware" }),
  aStackNode({ itemId: "n3", title: "Add login screen", parents: ["n2", "n1"] }),
];

describe("parentTitles", () => {
  it("names each parent by its title", () => {
    // Given
    const node = A_STACK[2];

    // When
    const titles = parentTitles(node, A_STACK);

    // Then
    expect(titles).toEqual(["Add auth middleware", "Add token store"]);
  });

  it("names the parents in the order the node lists them", () => {
    // Given — the first parent is the primary spine, so the order carries meaning
    const node = aStackNode({ itemId: "n3", parents: ["n1", "n2"] });

    // When
    const titles = parentTitles(node, A_STACK);

    // Then
    expect(titles).toEqual(["Add token store", "Add auth middleware"]);
  });

  it("skips a parent id no node in the plan carries", () => {
    // Given
    const node = aStackNode({ itemId: "n3", parents: ["n1", "gone"] });

    // When
    const titles = parentTitles(node, A_STACK);

    // Then
    expect(titles).toEqual(["Add token store"]);
  });

  it("names nothing for a root node", () => {
    // Given
    const node = A_STACK[0];

    // When
    const titles = parentTitles(node, A_STACK);

    // Then
    expect(titles).toEqual([]);
  });
});
