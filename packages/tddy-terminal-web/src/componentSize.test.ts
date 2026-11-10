import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { relative } from "node:path";

// Structural rule: every production source file in this package stays within the
// 500-line budget the oversized-file code issues measure against. Test files are
// not production code and are outside the budget.

const LINE_BUDGET = 500;
const PACKAGE_SOURCE_DIR = import.meta.dir;

type SourceFileSize = { path: string; lines: number };

function productionSourceFilesOf(dir: string): string[] {
  const glob = new Bun.Glob("**/*.{ts,tsx}");
  return [...glob.scanSync({ cwd: dir, absolute: true })].filter(
    (path) => !/\.test\.tsx?$/.test(path),
  );
}

function measure(path: string): SourceFileSize {
  const text = readFileSync(path, "utf8");
  const lines = text.endsWith("\n") ? text.split("\n").length - 1 : text.split("\n").length;
  return { path: relative(PACKAGE_SOURCE_DIR, path), lines };
}

function overBudget(files: SourceFileSize[]): string[] {
  return files
    .filter((file) => file.lines > LINE_BUDGET)
    .map((file) => `${file.path}: ${file.lines} lines (budget ${LINE_BUDGET})`)
    .sort();
}

describe("tddy-terminal-web source size", () => {
  it("keeps every production source file within the 500-line budget", () => {
    // Given
    const sourceFiles = productionSourceFilesOf(PACKAGE_SOURCE_DIR);

    // When
    const oversized = overBudget(sourceFiles.map(measure));

    // Then
    expect(sourceFiles.length).toBeGreaterThan(0);
    expect(oversized).toEqual([]);
  });
});
