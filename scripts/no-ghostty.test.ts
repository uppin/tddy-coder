import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/**
 * ghostty-web is gone from the web packages, and so is its name: the terminal components are
 * `TerminalView` / `TerminalSession`, and no source, spec, test id, Cypress command, story or
 * package manifest still says "ghostty".
 *
 * Scope: every tracked file of every JS workspace package (Markdown docs excepted — the change
 * history may name what was removed), plus the root `package.json` and `bun.lock`.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-08-drop-ghostty.md
 */

const REPO_ROOT = `${import.meta.dir}/..`;

function webWorkspacePackages(): string[] {
  const manifest = JSON.parse(readFileSync(`${REPO_ROOT}/package.json`, "utf8")) as { workspaces: string[] };
  return manifest.workspaces;
}

function trackedFilesOf(pathspecs: string[]): string[] {
  return execFileSync("git", ["ls-files", "--", ...pathspecs], { cwd: REPO_ROOT, encoding: "utf8" })
    .split("\n")
    .filter((path) => path !== "" && !path.endsWith(".md"));
}

function filesMentioningGhostty(pathspecs: string[]): string[] {
  try {
    return execFileSync("git", ["grep", "-l", "-i", "ghostty", "--", ...pathspecs], {
      cwd: REPO_ROOT,
      encoding: "utf8",
    })
      .split("\n")
      .filter((path) => path !== "" && !path.endsWith(".md"));
  } catch (error) {
    // git grep exits 1 when nothing matches; anything else is a real failure
    if ((error as { status?: number }).status === 1) return [];
    throw error;
  }
}

describe("ghostty-web is removed from the web packages", () => {
  test("no web package file mentions ghostty", () => {
    // Given
    const scope = [...webWorkspacePackages(), "package.json", "bun.lock"];

    // When
    const mentioning = filesMentioningGhostty(scope);

    // Then
    expect(trackedFilesOf(webWorkspacePackages()).length).toBeGreaterThan(0);
    expect(mentioning).toEqual([]);
  });

  test("no web package file is named after ghostty", () => {
    // Given
    const files = trackedFilesOf(webWorkspacePackages());

    // When
    const named = files.filter((path) => /ghostty/i.test(path.split("/").at(-1) ?? ""));

    // Then
    expect(named).toEqual([]);
  });
});
