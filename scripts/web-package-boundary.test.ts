import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { forbiddenImports } from "./web-package-boundary";

class APackage {
  readonly dir = mkdtempSync(join(tmpdir(), "boundary-"));
  withSource(path: string, body: string): this {
    const file = join(this.dir, "src", path);
    mkdirSync(join(file, ".."), { recursive: true });
    writeFileSync(file, body);
    return this;
  }
  dispose(): void {
    rmSync(this.dir, { recursive: true, force: true });
  }
}

let pkg: APackage | undefined;
afterEach(() => pkg?.dispose());

describe("forbidden imports", () => {
  test("finds a static import of a forbidden package", () => {
    // Given
    pkg = new APackage().withSource("a.tsx", 'import { Room } from "livekit-client";\n');

    // When
    const found = forbiddenImports(pkg.dir, ["livekit-client"]);

    // Then
    expect(found).toEqual([{ file: "src/a.tsx", specifier: "livekit-client" }]);
  });

  test("finds re-exports, type imports and dynamic imports in nested files", () => {
    // Given
    pkg = new APackage()
      .withSource("deep/b.ts", 'export * from "tddy-web/x";\nimport type { T } from "tddy-web";\nconst m = import("tddy-web/y");\n');

    // When
    const found = forbiddenImports(pkg.dir, ["tddy-web"]).map((f) => f.specifier);

    // Then
    expect(found.sort()).toEqual(["tddy-web", "tddy-web/x", "tddy-web/y"]);
  });

  test("a rule matches the package and its subpaths, not a package sharing its prefix", () => {
    // Given
    pkg = new APackage().withSource("c.ts", 'import "tddy-web-extra";\nimport "tddy-web/z";\n');

    // When
    const found = forbiddenImports(pkg.dir, ["tddy-web"]).map((f) => f.specifier);

    // Then
    expect(found).toEqual(["tddy-web/z"]);
  });

  test("a RegExp rule matches relative paths", () => {
    // Given
    pkg = new APackage().withSource("d.ts", 'import { X } from "../../tddy-web/src/gen/x_pb";\n');

    // When
    const found = forbiddenImports(pkg.dir, [/(^|\/)gen\//]);

    // Then
    expect(found).toHaveLength(1);
  });

  test("a clean package has no forbidden imports", () => {
    // Given
    pkg = new APackage().withSource("e.ts", 'import React from "react";\n');

    // When / Then
    expect(forbiddenImports(pkg.dir, ["livekit-client", "tddy-web"])).toEqual([]);
  });
});
