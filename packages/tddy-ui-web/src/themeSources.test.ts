import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const PACKAGES = join(import.meta.dir, "../..");
const THEME = readFileSync(join(import.meta.dir, "theme.css"), "utf8");

/** Every `@source` path the theme names. */
function sources(): string[] {
  return [...THEME.matchAll(/@source\s+"([^"]+)"/g)].map((m) => m[1]);
}

/** Packages whose `src/` renders JSX with Tailwind classes — the ones the app's stylesheet must scan. */
function packagesThatRenderClasses(): string[] {
  const rendersClasses = (dir: string): boolean =>
    readdirSync(dir, { withFileTypes: true }).some((entry) =>
      entry.isDirectory()
        ? entry.name !== "node_modules" && entry.name !== "gen" && rendersClasses(join(dir, entry.name))
        : entry.name.endsWith(".tsx") && readFileSync(join(dir, entry.name), "utf8").includes("className"),
    );
  return readdirSync(PACKAGES)
    .filter((name) => name.startsWith("tddy-") && name !== "tddy-ui-web")
    .filter((name) => existsSync(join(PACKAGES, name, "src", ".")) && existsSync(join(PACKAGES, name, "package.json")))
    .filter((name) => rendersClasses(join(PACKAGES, name, "src")))
    .sort();
}

describe("the theme's Tailwind sources", () => {
  test("name no wildcard directory, which Tailwind does not expand", () => {
    // Then
    expect(sources().filter((path) => path.includes("*"))).toEqual([]);
  });

  test("cover every package that renders Tailwind classes", () => {
    // Given
    const named = new Set(sources());

    // When
    const missing = packagesThatRenderClasses().filter((name) => !named.has(`../../${name}/src`));

    // Then
    expect(missing).toEqual([]);
  });
});
