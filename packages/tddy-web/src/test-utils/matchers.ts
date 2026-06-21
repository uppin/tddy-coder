/**
 * Domain-specific matchers for the bun:test suite.
 *
 * Import this module (or the index barrel) once in a test file to register
 * the matchers globally. Subsequent `expect(...)` calls gain the extended API.
 *
 * For bun:test only — do NOT share with Cypress (which uses chai/sinon).
 *
 * Usage:
 *   import "../test-utils/matchers"; // side-effect: registers matchers
 *   // or:
 *   import { toContainSessionIdsInOrder } from "../test-utils";
 */

import { expect } from "bun:test";

// ---------------------------------------------------------------------------
// Matcher implementations
// ---------------------------------------------------------------------------

expect.extend({
  /**
   * Asserts that an array of objects with a `sessionId` string field contains
   * exactly the given IDs in the given order.
   *
   * @example
   *   expect(sortedSessions).toContainSessionIdsInOrder(["a", "b", "c"]);
   */
  toContainSessionIdsInOrder(
    received: unknown,
    expected: string[],
  ): { pass: boolean; message: () => string } {
    const actual = Array.isArray(received)
      ? (received as { sessionId: string }[]).map((s) => s.sessionId)
      : [];
    const pass = JSON.stringify(actual) === JSON.stringify(expected);
    return {
      pass,
      message: () =>
        `expected session IDs\n  ${JSON.stringify(actual)}\n${pass ? "not " : ""}to equal\n  ${JSON.stringify(expected)}`,
    };
  },

  /**
   * Asserts that the value equals `{ checked: false, indeterminate: true }` —
   * the shape returned by `computeHeaderCheckboxState` when selection is partial.
   *
   * @example
   *   expect(computeHeaderCheckboxState(1, 3)).toBeIndeterminateHeaderState();
   */
  toBeIndeterminateHeaderState(
    received: unknown,
  ): { pass: boolean; message: () => string } {
    const r = received as { checked?: boolean; indeterminate?: boolean } | null;
    const pass =
      r !== null &&
      typeof r === "object" &&
      r.checked === false &&
      r.indeterminate === true;
    return {
      pass,
      message: () =>
        `expected ${JSON.stringify(received)} ${pass ? "not " : ""}to be an indeterminate header state { checked: false, indeterminate: true }`,
    };
  },

  /**
   * Asserts that the value equals `{ checked: true, indeterminate: false }` —
   * the shape returned by `computeHeaderCheckboxState` when all rows are selected.
   *
   * @example
   *   expect(computeHeaderCheckboxState(3, 3)).toBeCheckedHeaderState();
   */
  toBeCheckedHeaderState(
    received: unknown,
  ): { pass: boolean; message: () => string } {
    const r = received as { checked?: boolean; indeterminate?: boolean } | null;
    const pass =
      r !== null &&
      typeof r === "object" &&
      r.checked === true &&
      r.indeterminate === false;
    return {
      pass,
      message: () =>
        `expected ${JSON.stringify(received)} ${pass ? "not " : ""}to be a checked header state { checked: true, indeterminate: false }`,
    };
  },
});

// ---------------------------------------------------------------------------
// TypeScript augmentation so IDEs and type-checks recognise the custom matchers
// ---------------------------------------------------------------------------

declare module "bun:test" {
  interface Matchers<R> {
    /**
     * Asserts that an array of session-like objects contains exactly the given
     * session IDs in the given order.
     */
    toContainSessionIdsInOrder(expected: string[]): R;

    /**
     * Asserts `{ checked: false, indeterminate: true }` — partial selection
     * in a session table header checkbox.
     */
    toBeIndeterminateHeaderState(): R;

    /**
     * Asserts `{ checked: true, indeterminate: false }` — all rows selected
     * in a session table header checkbox.
     */
    toBeCheckedHeaderState(): R;
  }
}

// Re-export a no-op symbol so callers can `import { matchers } from "../test-utils/matchers"` as
// a named side-effect import without needing `import "../test-utils/matchers"` syntax.
export const matchers = "registered" as const;
