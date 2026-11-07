import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { readWorkspace } from "./web-affected";

/**
 * Every JS workspace package has its own PR check, and every check belongs to a package: the
 * pairing is what keeps a package added later from shipping untested.
 */

const REPO_ROOT = join(import.meta.dir, "..");
const WORKFLOWS = join(REPO_ROOT, ".github", "workflows");
const CALLER = /^web-(.+)\.yml$/;

function webWorkflowCallers(): Map<string, string> {
  const callers = new Map<string, string>();
  for (const file of readdirSync(WORKFLOWS)) {
    const match = CALLER.exec(file);
    if (match && file !== "web-package.yml") {
      callers.set(match[1], readFileSync(join(WORKFLOWS, file), "utf8"));
    }
  }
  return callers;
}

describe("web package workflows", () => {
  test("every workspace package has exactly one web workflow", () => {
    // Given
    const packages = readWorkspace(REPO_ROOT).packages.map((p) => p.name);

    // When
    const missing = packages.filter((name) => !webWorkflowCallers().has(name));

    // Then
    expect(missing).toEqual([]);
  });

  test("every web workflow names a workspace package and calls the shared workflow", () => {
    // Given
    const packages = new Set(readWorkspace(REPO_ROOT).packages.map((p) => p.name));

    // When
    const strays = [...webWorkflowCallers()]
      .filter(
        ([name, body]) =>
          !packages.has(name) ||
          !body.includes("uses: ./.github/workflows/web-package.yml") ||
          !body.includes(`package: ${name}`),
      )
      .map(([name]) => name);

    // Then
    expect(strays).toEqual([]);
  });

  test("the shared workflow exists and ci.yml no longer runs the web suites", () => {
    // Given
    const ci = readFileSync(join(WORKFLOWS, "ci.yml"), "utf8");

    // When
    const shared = readdirSync(WORKFLOWS).includes("web-package.yml");

    // Then
    expect(shared).toBe(true);
    expect(ci).not.toMatch(/^\s{2}web:\s*$/m);
  });
});
