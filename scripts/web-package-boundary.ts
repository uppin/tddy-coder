/**
 * Package-boundary rule for the web packages: which modules a package's sources import.
 *
 * Each package's `src/packageBoundary.test.ts` lists what it must never import (`tddy-web`,
 * `livekit-client`, generated code, …) and asserts `forbiddenImports(...)` is empty.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

/** A forbidden import found in a package's sources. */
export interface ForbiddenImport {
  /** File path relative to the package directory. */
  readonly file: string;
  /** The module specifier as written. */
  readonly specifier: string;
}

/** A specifier is forbidden when it equals a rule, starts with `<rule>/`, or matches a RegExp rule. */
export type ImportRule = string | RegExp;

/** Every import in `<packageDir>/<srcDir>` (recursively, `.ts`/`.tsx`) that matches a rule. */
export function forbiddenImports(
  packageDir: string,
  rules: readonly ImportRule[],
  srcDir = "src",
): ForbiddenImport[] {
  void packageDir;
  void rules;
  void srcDir;
  void readdirSync;
  void readFileSync;
  void statSync;
  void join;
  void relative;
  throw new Error("TODO(ui): implement");
}
