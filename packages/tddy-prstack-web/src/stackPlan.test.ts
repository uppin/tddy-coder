import { describe, expect, it } from "bun:test";
import { parseStackPlan } from "./stackPlan";

/**
 * Tests for `parseStackPlan`'s handling of a node's persisted row position.
 *
 * `display_order` is additive on the wire and omitted entirely for a node that carries none, so the
 * parser has to distinguish "position 0" from "no position at all" — the two drive different
 * orderings, and reading an absent field as `0` would put every legacy node first.
 */

/** The `stack_plan_json` wire shape: snake_case, matching the Rust struct's default serde encoding. */
function aWirePlan(nodes: Record<string, unknown>[]): string {
  return JSON.stringify({ version: 1, nodes });
}

describe("parseStackPlan", () => {
  it("parses a node's persisted row position", () => {
    // Given
    const json = aWirePlan([{ node_id: "n1", title: "Add token store", display_order: 2 }]);

    // When
    const stack = parseStackPlan(json);

    // Then
    expect(stack.nodes[0].displayOrder).toBe(2);
  });

  it("parses a first position as a position, not as an absent one", () => {
    // Given — zero is a real position and must not read as "unset"
    const json = aWirePlan([{ node_id: "n1", title: "Add token store", display_order: 0 }]);

    // When
    const stack = parseStackPlan(json);

    // Then
    expect(stack.nodes[0].displayOrder).toBe(0);
  });

  it("leaves the position absent for a node the plan does not order", () => {
    // Given — a plan authored before display order existed omits the field entirely
    const json = aWirePlan([{ node_id: "n1", title: "Add token store" }]);

    // When
    const stack = parseStackPlan(json);

    // Then
    expect(stack.nodes[0].displayOrder).toBeNull();
  });

  it("leaves the position absent for a node whose order is explicitly null", () => {
    // Given
    const json = aWirePlan([{ node_id: "n1", title: "Add token store", display_order: null }]);

    // When
    const stack = parseStackPlan(json);

    // Then
    expect(stack.nodes[0].displayOrder).toBeNull();
  });
});
