/**
 * Custom matchers for the tddy-session-web bun:test suites. Registered as a side effect of importing
 * `./builders`. For bun:test only — never import from Cypress tests.
 */

import { expect } from "bun:test";

expect.extend({
  /** Asserts that an array of objects with a `sessionId` field holds exactly the given ids, in order. */
  toContainSessionIdsInOrder(received: unknown, expected: string[]): { pass: boolean; message: () => string } {
    const actual = Array.isArray(received) ? (received as { sessionId: string }[]).map((s) => s.sessionId) : [];
    const pass = JSON.stringify(actual) === JSON.stringify(expected);
    return {
      pass,
      message: () =>
        `expected session IDs\n  ${JSON.stringify(actual)}\n${pass ? "not " : ""}to equal\n  ${JSON.stringify(expected)}`,
    };
  },
});

declare module "bun:test" {
  interface Matchers<R> {
    toContainSessionIdsInOrder(expected: string[]): R;
  }
}
