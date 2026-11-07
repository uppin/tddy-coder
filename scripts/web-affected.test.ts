import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  affectedPackages,
  GLOBAL_INPUTS,
  readWorkspace,
  runCli,
  type Workspace,
} from "./web-affected";

/**
 * A miniature bun workspace on disk: a root manifest listing `workspaces`, and one directory per
 * package whose `package.json` names its workspace dependencies — the two things the detector reads.
 */
class AWorkspaceRepo {
  readonly root = mkdtempSync(join(tmpdir(), "web-affected-"));
  private readonly members: string[] = [];

  withPackage(
    name: string,
    deps: { dependencies?: string[]; devDependencies?: string[] } = {},
  ): this {
    const dir = `packages/${name}`;
    mkdirSync(join(this.root, dir), { recursive: true });
    const asWorkspaceDeps = (names: string[] = []) =>
      Object.fromEntries(names.map((n) => [n, "workspace:*"]));
    writeFileSync(
      join(this.root, dir, "package.json"),
      JSON.stringify({
        name,
        dependencies: { react: "^19.0.0", ...asWorkspaceDeps(deps.dependencies) },
        devDependencies: asWorkspaceDeps(deps.devDependencies),
      }),
    );
    this.members.push(dir);
    this.writeRoot();
    return this;
  }

  workspace(): Workspace {
    return readWorkspace(this.root);
  }

  private writeRoot(): void {
    writeFileSync(
      join(this.root, "package.json"),
      JSON.stringify({ private: true, workspaces: this.members }),
    );
  }

  dispose(): void {
    rmSync(this.root, { recursive: true, force: true });
  }
}

/** `transport ← web ← desktop`, plus an unrelated `testkit`: the shapes every case below needs. */
function aLayeredWorkspace(): AWorkspaceRepo {
  return new AWorkspaceRepo()
    .withPackage("transport")
    .withPackage("web", { dependencies: ["transport"] })
    .withPackage("desktop", { dependencies: ["web"] })
    .withPackage("testkit");
}

let repo: AWorkspaceRepo | undefined;
afterEach(() => repo?.dispose());

describe("affected web packages", () => {
  test("a change inside one package affects that package", () => {
    // Given
    repo = aLayeredWorkspace();

    // When
    const affected = affectedPackages(["packages/testkit/src/index.ts"], repo.workspace());

    // Then
    expect([...affected]).toEqual(["testkit"]);
  });

  test("a change in a package affects every package that depends on it, transitively", () => {
    // Given
    repo = aLayeredWorkspace();

    // When
    const affected = affectedPackages(["packages/transport/src/transport.ts"], repo.workspace());

    // Then
    expect([...affected].sort()).toEqual(["desktop", "transport", "web"]);
  });

  test("a change in a dependent does not affect its dependency", () => {
    // Given
    repo = aLayeredWorkspace();

    // When
    const affected = affectedPackages(["packages/desktop/src/main.ts"], repo.workspace());

    // Then
    expect([...affected]).toEqual(["desktop"]);
  });

  test("a change outside every package affects nothing", () => {
    // Given
    repo = aLayeredWorkspace();

    // When
    const affected = affectedPackages(
      ["packages/tddy-core/src/lib.rs", "docs/dev/guides/ci.md"],
      repo.workspace(),
    );

    // Then
    expect([...affected]).toEqual([]);
  });

  test("a package directory is matched whole, not by name prefix", () => {
    // Given a package whose name is a prefix of another's
    repo = new AWorkspaceRepo().withPackage("web").withPackage("web-extra");

    // When
    const affected = affectedPackages(["packages/web-extra/src/a.ts"], repo.workspace());

    // Then
    expect([...affected]).toEqual(["web-extra"]);
  });

  for (const globalInput of GLOBAL_INPUTS) {
    test(`a change to ${globalInput} affects every package`, () => {
      // Given
      repo = aLayeredWorkspace();

      // When
      const affected = affectedPackages([globalInput], repo.workspace());

      // Then
      expect([...affected].sort()).toEqual(["desktop", "testkit", "transport", "web"]);
    });
  }

  test("devDependencies on a workspace package count as dependencies", () => {
    // Given a package that only uses the testkit in its tests
    repo = new AWorkspaceRepo()
      .withPackage("testkit")
      .withPackage("web", { devDependencies: ["testkit"] });

    // When
    const affected = affectedPackages(["packages/testkit/src/backend.ts"], repo.workspace());

    // Then
    expect([...affected].sort()).toEqual(["testkit", "web"]);
  });
});

describe("the workspace reader", () => {
  test("lists each member with its workspace dependencies and nothing else", () => {
    // Given
    repo = aLayeredWorkspace();

    // When
    const web = repo.workspace().packages.find((p) => p.name === "web");

    // Then the registry dependency on react is not a workspace dependency
    expect(web).toEqual({ name: "web", dir: "packages/web", workspaceDeps: ["transport"] });
  });
});

describe("the CLI", () => {
  test("prints affected=true for a package the diff affects", () => {
    // Given
    repo = aLayeredWorkspace();
    const diff = () => ["packages/transport/src/a.ts"];

    // When
    const line = runCli(["--package", "web", "--base", "b", "--head", "h"], repo.root, diff);

    // Then
    expect(line).toBe("affected=true");
  });

  test("prints affected=false for a package the diff does not affect", () => {
    // Given
    repo = aLayeredWorkspace();
    const diff = () => ["packages/desktop/src/a.ts"];

    // When
    const line = runCli(["--package", "transport", "--base", "b", "--head", "h"], repo.root, diff);

    // Then
    expect(line).toBe("affected=false");
  });

  test("--all marks the package affected without reading a diff", () => {
    // Given
    repo = aLayeredWorkspace();
    const diff = (): readonly string[] => {
      throw new Error("a push to master must not diff");
    };

    // When
    const line = runCli(["--package", "testkit", "--all"], repo.root, diff);

    // Then
    expect(line).toBe("affected=true");
  });

  test("rejects a package that is not in the workspace", () => {
    // Given
    repo = aLayeredWorkspace();

    // When / Then
    expect(() =>
      runCli(["--package", "nope", "--base", "b", "--head", "h"], repo!.root, () => []),
    ).toThrow(/nope/);
  });
});
