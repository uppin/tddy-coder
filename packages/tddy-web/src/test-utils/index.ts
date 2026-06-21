/**
 * Test utilities barrel for the bun:test suite.
 *
 * Importing this module registers the custom matchers as a side-effect and
 * re-exports all builders so a single import covers everything:
 *
 *   import { aSessionEntry, anActiveSession, aViewRect } from "../test-utils";
 *
 * For bun:test only — never import from Cypress tests.
 */

export * from "./builders";
export * from "./matchers";
