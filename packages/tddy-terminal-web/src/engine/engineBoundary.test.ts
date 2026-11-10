import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, relative } from "node:path";

// The engine boundary: the `@xterm/*` packages are reached only from the xterm engine. Nothing else
// imports the library or casts a terminal to reach its members.
//
// PRD: packages/tddy-terminal-web/docs/terminal-engine.md (acceptance criterion 1)

const PACKAGE_SOURCE_DIR = dirname(import.meta.dir);
const ENGINE_FILES = ["engine/xtermEngine.ts"];

/** Members of xterm.js's `Terminal` that only the engine may reach through an `as unknown as` cast. */
const XTERM_MEMBERS = ["_core", "_renderService", "buffer", "parser", "modes", "unicode"];

type SourceFile = { path: string; text: string };

function productionSourcesOutsideTheEngine(): SourceFile[] {
  const glob = new Bun.Glob("**/*.{ts,tsx}");
  return [...glob.scanSync({ cwd: PACKAGE_SOURCE_DIR, absolute: true })]
    .map((absolute) => ({ absolute, path: relative(PACKAGE_SOURCE_DIR, absolute) }))
    .filter(({ path }) => !/\.test\.tsx?$/.test(path) && !ENGINE_FILES.includes(path))
    .map(({ absolute, path }) => ({ path, text: readFileSync(absolute, "utf8") }));
}

function importingXterm(files: SourceFile[]): string[] {
  const xtermImport = /\bfrom\s+["']@xterm\/[^"']*["']|\bimport\(\s*["']@xterm\//;
  return files.filter((file) => xtermImport.test(file.text)).map((file) => file.path).sort();
}

function castingToXtermMembers(files: SourceFile[]): string[] {
  const castBlock = /as\s+unknown\s+as\s*\{[^}]*\}/g;
  return files
    .filter((file) =>
      [...file.text.matchAll(castBlock)].some(([cast]) =>
        XTERM_MEMBERS.some((member) => new RegExp(`\\b${member}\\b`).test(cast)),
      ),
    )
    .map((file) => file.path)
    .sort();
}

describe("terminal engine boundary", () => {
  it("lets only the xterm engine import @xterm packages", () => {
    // Given
    const files = productionSourcesOutsideTheEngine();

    // When
    const importers = importingXterm(files);

    // Then
    expect(files.length).toBeGreaterThan(0);
    expect(importers).toEqual([]);
  });

  it("leaves no cast that reaches an xterm.js member outside the engine", () => {
    // Given
    const files = productionSourcesOutsideTheEngine();

    // When
    const casters = castingToXtermMembers(files);

    // Then
    expect(casters).toEqual([]);
  });
});
