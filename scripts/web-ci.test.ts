import { describe, expect, test } from "bun:test";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { type CiPackage, lintGaps, planOutputs, planShard, readCiPackages, readLintExclusions } from "./web-ci";

const REPO_ROOT = join(import.meta.dir, "..");

function pkg(name: string, extra: Partial<CiPackage> = {}): CiPackage {
  return { name, dir: `packages/${name}`, scripts: new Set(["test:ci"]), needs: [], componentSpecs: [], ...extra };
}

describe("planShard", () => {
  test("deals unit suites whole by package and component specs one by one", () => {
    // Given
    const packages = [
      pkg("a", { componentSpecs: ["cypress/component/1.cy.tsx", "cypress/component/2.cy.tsx"] }),
      pkg("b", { componentSpecs: ["cypress/component/3.cy.tsx"] }),
      pkg("c"),
    ];
    const affected = new Set(["a", "b", "c"]);

    // When
    const shards = [1, 2, 3].map((s) => planShard(packages, affected, s, 3));

    // Then
    expect(shards.map((s) => s.unit)).toEqual([["a"], ["b"], ["c"]]);
    expect(shards.map((s) => s.component)).toEqual([
      { a: ["cypress/component/1.cy.tsx"] },
      { a: ["cypress/component/2.cy.tsx"] },
      { b: ["cypress/component/3.cy.tsx"] },
    ]);
  });

  test("every affected suite lands in exactly one shard", () => {
    // Given
    const packages = readCiPackages(REPO_ROOT);
    const all = new Set(packages.map((p) => p.name));

    // When
    const shards = [1, 2, 3].map((s) => planShard(packages, all, s, 3));

    // Then
    const units = shards.flatMap((s) => s.unit).sort();
    const specs = shards.flatMap((s) => Object.entries(s.component).flatMap(([p, xs]) => xs.map((x) => `${p}/${x}`)));
    expect(units).toEqual(packages.filter((p) => p.scripts.has("test:ci")).map((p) => p.name).sort());
    expect(new Set(specs).size).toBe(specs.length);
    expect(specs.length).toBe(packages.reduce((n, p) => n + p.componentSpecs.length, 0));
  });

  test("an unaffected package contributes nothing and a shard with nothing to run is not affected", () => {
    // Given
    const packages = [pkg("a", { needs: ["tddy-coder"] }), pkg("b")];

    // When
    const plan = planShard(packages, new Set(["b"]), 2, 3);

    // Then
    expect(plan).toEqual({ shard: 2, of: 3, unit: [], component: {}, needs: [] });
    expect(planOutputs(plan)).toContain("affected=false");
  });

  test("a shard needs only what its own share needs", () => {
    // Given
    const packages = [pkg("a", { needs: ["tddy-coder"] }), pkg("b", { needs: ["livekit-image"] })];

    // When
    const first = planShard(packages, new Set(["a", "b"]), 1, 2);

    // Then
    expect(first.needs).toEqual(["tddy-coder"]);
    expect(planOutputs(first)).toEqual(
      expect.arrayContaining(["affected=true", "cypress=false", "needs-tddy-coder=true", "needs-livekit-image=false"]),
    );
  });

  test("rejects a shard outside 1..of", () => {
    expect(() => planShard([], new Set(), 4, 3)).toThrow();
  });
});

describe("lint exclusions", () => {
  test("every package without a typecheck script is listed, and only those", () => {
    // When
    const gaps = lintGaps(readCiPackages(REPO_ROOT), readLintExclusions(REPO_ROOT));

    // Then
    expect(gaps).toEqual({ unlisted: [], stale: [] });
  });

  test("an unlisted package without a typecheck script and an entry for a missing package are reported", () => {
    // Given
    const packages = [pkg("typed", { scripts: new Set(["typecheck"]) }), pkg("untyped")];

    // When
    const gaps = lintGaps(packages, [
      { package: "typed", reason: "fails today", todo: "docs/dev/todo/x.md" },
      { package: "gone", reason: "", todo: null },
    ]);

    // Then
    expect(gaps).toEqual({ unlisted: ["untyped"], stale: ["gone"] });
  });

  test("every exclusion with typecheck debt names an existing todo record", () => {
    // When
    const missing = readLintExclusions(REPO_ROOT)
      .filter((e) => e.todo !== null && !existsSync(join(REPO_ROOT, e.todo)))
      .map((e) => e.package);

    // Then
    expect(missing).toEqual([]);
  });
});
