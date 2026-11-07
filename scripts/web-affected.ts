/**
 * Which JS workspace packages a change affects — the decision behind each `web-<package>.yml` PR check.
 *
 * A package is affected when a file under its directory changed, when a workspace package it depends
 * on (`workspace:*` in `dependencies` or `devDependencies`, transitively) is affected, or when a
 * global input changed — the lockfile, the root manifest, the toolchain, or the CI machinery itself.
 *
 * CLI (used by `.github/workflows/web-package.yml`):
 *
 *   bun scripts/web-affected.ts --package <name> --base <sha> --head <sha>
 *   bun scripts/web-affected.ts --package <name> --all
 *
 * prints `affected=true` or `affected=false`, the line the workflow appends to `$GITHUB_OUTPUT`.
 */

/** One workspace package, as the root `workspaces` list and its own `package.json` describe it. */
export interface WorkspacePackage {
  /** `name` from the package's `package.json`. */
  readonly name: string;
  /** Directory relative to the repository root, e.g. `packages/tddy-web`, no trailing slash. */
  readonly dir: string;
  /** Names of the workspace packages it depends on (`workspace:*` in dependencies or devDependencies). */
  readonly workspaceDeps: readonly string[];
}

export interface Workspace {
  readonly packages: readonly WorkspacePackage[];
}

/** Paths, relative to the repository root, whose change affects every package. */
export const GLOBAL_INPUTS: readonly string[] = [
  "package.json",
  "bun.lock",
  "bunfig.toml",
  "flake.nix",
  "flake.lock",
  ".github/workflows/web-package.yml",
  ".github/workflows/build-gate.yml",
  "scripts/web-affected.ts",
];

/** Read the workspace rooted at `root`: the root `workspaces` list and each member's `package.json`. */
export function readWorkspace(root: string): Workspace {
  void root;
  throw new Error("TODO(web-ci): implement");
}

/** Names of the packages in `workspace` that `changedFiles` (repo-relative paths) affect. */
export function affectedPackages(
  changedFiles: readonly string[],
  workspace: Workspace,
): Set<string> {
  void changedFiles;
  void workspace;
  throw new Error("TODO(web-ci): implement");
}

/**
 * Run the CLI against the repository at `root`; returns the line to print.
 * `changedFilesBetween` lists the files changed between two commits (git, injected for tests).
 */
export function runCli(
  argv: readonly string[],
  root: string,
  changedFilesBetween: (base: string, head: string) => readonly string[],
): string {
  void argv;
  void root;
  void changedFilesBetween;
  throw new Error("TODO(web-ci): implement");
}

if (import.meta.main) {
  // TODO(web-ci): implement — wire runCli to process.argv and `git diff --name-only base...head`.
  throw new Error("TODO(web-ci): implement");
}
