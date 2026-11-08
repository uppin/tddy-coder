import { describe, it, expect } from "bun:test";
import { readFileSync } from "node:fs";
import { dirname, relative } from "node:path";

// The engine boundary: ghostty-web is reached only from the ghostty engine (and the WASM instance
// loader it uses). Nothing else imports the library or casts a terminal to reach its members.
//
// PRD: packages/tddy-terminal-web/docs/terminal-engine.md (acceptance criterion 1)

const PACKAGE_SOURCE_DIR = dirname(import.meta.dir);
const ENGINE_FILES = ["engine/ghosttyEngine.ts", "lib/ghosttyInstance.ts"];

/** Members only ghostty-web's `Terminal` has, reached today through `as unknown as` casts. */
const GHOSTTY_MEMBERS = ["wasmTerm", "getViewportY", "getScrollbackLength", "hasMouseTracking", "devicePixelRatio"];

type SourceFile = { path: string; text: string };

function productionSourcesOutsideTheEngine(): SourceFile[] {
  const glob = new Bun.Glob("**/*.{ts,tsx}");
  return [...glob.scanSync({ cwd: PACKAGE_SOURCE_DIR, absolute: true })]
    .map((absolute) => ({ absolute, path: relative(PACKAGE_SOURCE_DIR, absolute) }))
    .filter(({ path }) => !/\.test\.tsx?$/.test(path) && !ENGINE_FILES.includes(path))
    .map(({ absolute, path }) => ({ path, text: readFileSync(absolute, "utf8") }));
}

function importingGhosttyWeb(files: SourceFile[]): string[] {
  const ghosttyImport = /\bfrom\s+["']ghostty-web(\/[^"']*)?["']|\bimport\(\s*["']ghostty-web/;
  return files.filter((file) => ghosttyImport.test(file.text)).map((file) => file.path).sort();
}

function castingToGhosttyMembers(files: SourceFile[]): string[] {
  const castBlock = /as\s+unknown\s+as\s*\{[^}]*\}/g;
  return files
    .filter((file) =>
      [...file.text.matchAll(castBlock)].some(([cast]) =>
        GHOSTTY_MEMBERS.some((member) => new RegExp(`\\b${member}\\b`).test(cast)),
      ),
    )
    .map((file) => file.path)
    .sort();
}

describe("terminal engine boundary", () => {
  it("lets only the ghostty engine import ghostty-web", () => {
    // Given
    const files = productionSourcesOutsideTheEngine();

    // When
    const importers = importingGhosttyWeb(files);

    // Then
    expect(files.length).toBeGreaterThan(0);
    expect(importers).toEqual([]);
  });

  it("leaves no cast that reaches a ghostty-web member outside the engine", () => {
    // Given
    const files = productionSourcesOutsideTheEngine();

    // When
    const casters = castingToGhosttyMembers(files);

    // Then
    expect(casters).toEqual([]);
  });
});
