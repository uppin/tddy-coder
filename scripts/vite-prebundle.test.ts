import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { prebundledDeps } from "./vite-prebundle";

const pkg = (name: string) => join(import.meta.dir, "..", "packages", name);

describe("prebundledDeps", () => {
  test("pre-bundles a dependency the package resolves itself by its plain name", () => {
    expect(prebundledDeps(pkg("tddy-react-commons")).include).toContain("rxjs");
  });

  test("pre-bundles a workspace dependency's own dependency through that workspace package", () => {
    const { include } = prebundledDeps(pkg("tddy-web"));

    expect(include).toContain("tddy-react-commons > rxjs");
    expect(include).toContain("tddy-react-commons > dequal");
  });

  test("scans the sources of the workspace packages it imports, so none of their dependencies is met mid-spec", () => {
    expect(prebundledDeps(pkg("tddy-web")).entries).toContain(join(pkg("tddy-session-web"), "src/**/*.{ts,tsx}"));
  });

  test("keeps bun tests out of the scan, because their bun:test import aborts it", () => {
    expect(prebundledDeps(pkg("tddy-web")).entries).toContain("!**/*.test.{ts,tsx}");
  });
});
