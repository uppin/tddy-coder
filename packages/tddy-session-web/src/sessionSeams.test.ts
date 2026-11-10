import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { join } from "node:path";

/**
 * Session code reaches the app only through seams: the app location through `tddy-client-web`'s
 * navigation port, the app shell and the start-passphrase prompt through slots the app fills. Until the
 * session modules move (#860–#862), they still sit in tddy-web — this rule pins the seams there first.
 */

const REPO_ROOT = join(import.meta.dir, "../../..");
const SESSION_DIRS = ["packages/tddy-web/src/components/sessions/**", "packages/tddy-web/src/components/session/**", "packages/tddy-session-web/src/**"];

function importersOf(pattern: string): string[] {
  try {
    return execFileSync("git", ["grep", "-l", "-E", pattern, "--", ...SESSION_DIRS], { cwd: REPO_ROOT, encoding: "utf8" })
      .split("\n")
      .filter((f) => f && !f.endsWith(".test.ts") && !f.endsWith(".test.tsx"));
  } catch {
    return [];
  }
}

describe("session seams", () => {
  test("session code reads the app location only through the navigation port", () => {
    // When
    const direct = importersOf(String.raw`from ['"][./]*(\.\./)+routing/(useAppLocation|appRoutes)['"]`);

    // Then
    expect(direct).toEqual([]);
  });

  test("session code renders the app shell only through a slot", () => {
    // When
    const direct = importersOf(String.raw`from ['"][./]*(\.\./)+shell/AppShell['"]`);

    // Then
    expect(direct).toEqual([]);
  });

  test("session code shows the start-passphrase prompt only through its slot", () => {
    // When
    const direct = importersOf(String.raw`HostStartPassphrasePrompt['"]`);

    // Then
    expect(direct).toEqual([]);
  });
});
