import { describe, expect, test } from "bun:test";
import { Glob } from "bun";
import { join } from "node:path";

/**
 * tddy-web is the wiring package: the app entry, the app composition (`app/`), routing, the shell and
 * the `*AppPage` pages that place a package's screen in it. Feature components live in their packages.
 */

const SRC = import.meta.dir;
const WIRING = [
  /^app\//,
  /^routing\//,
  /^components\/shell\//,
  /(^|\/)[A-Za-z]+AppPage\.tsx$/,
  /^index\.tsx$/,
  /^index\.css$/,
  /^buildId\.ts$/,
  /^(vite-env|debug)\.d\.ts$/,
  /^test-utils\//,
  /\.test\.tsx?$/,
];

describe("tddy-web is wiring only", () => {
  test("holds no feature component or feature module", () => {
    // When
    const features = [...new Glob("**/*.{ts,tsx,css}").scanSync(SRC)].filter((f) => !WIRING.some((rule) => rule.test(f)));

    // Then
    expect(features.sort()).toEqual([]);
  });

  test("composes the app through the app container", () => {
    // Given
    const entry = Bun.file(join(SRC, "index.tsx"));

    // When
    const source = entry.text();

    // Then
    return source.then((text) => expect(text).toMatch(/composeApp\(\s*appHandler\(/));
  });
});
