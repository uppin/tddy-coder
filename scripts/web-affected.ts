/**
 * Which JS workspace packages a change affects — the decision behind the `Web tests` shards and the
 * `Storybook build` check.
 *
 * A package is affected when a file under its directory changed, when a workspace package it depends
 * on (`workspace:*` in `dependencies` or `devDependencies`, transitively) is affected, or when a
 * global input changed — the lockfile, the root manifest, the
 * toolchain, or the CI machinery itself.
 *
 * CLI (`.github/workflows/storybook.yml`; the shards plan through `scripts/web-ci.ts`):
 *
 *   bun scripts/web-affected.ts --package <name> --base <sha> --head <sha>
 *   bun scripts/web-affected.ts --package <name> --all
 *
 * prints `affected=true` or `affected=false`, the line the workflow appends to `$GITHUB_OUTPUT`.
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

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
  ".github/workflows/web.yml",
  ".github/workflows/storybook.yml",
  ".github/workflows/build-gate.yml",
  "scripts/web-affected.ts",
  "scripts/web-ci.ts",
  // Every package's vite.config.ts pre-bundles through it.
  "scripts/vite-prebundle.ts",
  // Every package's boundary rule test runs it.
  "scripts/web-package-boundary.ts",
];

/** The subset of a `package.json` this script reads. */
interface PackageManifest {
  readonly name?: unknown;
  readonly workspaces?: unknown;
  readonly dependencies?: Record<string, string>;
  readonly devDependencies?: Record<string, string>;
}

/** Read the workspace rooted at `root`: the root `workspaces` list and each member's `package.json`. */
export function readWorkspace(root: string): Workspace {
  const rootManifestPath = join(root, "package.json");
  const members = readManifest(rootManifestPath).workspaces;
  if (!Array.isArray(members) || !members.every((m): m is string => typeof m === "string")) {
    throw new Error(`${rootManifestPath}: "workspaces" must be a list of package directories`);
  }
  return { packages: members.map((member) => readMember(root, member)) };
}

/** Names of the packages in `workspace` that `changedFiles` (repo-relative paths) affect. */
export function affectedPackages(
  changedFiles: readonly string[],
  workspace: Workspace,
): Set<string> {
  if (changedFiles.some((file) => GLOBAL_INPUTS.includes(file))) {
    return new Set(workspace.packages.map((pkg) => pkg.name));
  }

  const affected = new Set<string>();
  for (const pkg of workspace.packages) {
    const touched = changedFiles.some((file) => file.startsWith(`${pkg.dir}/`));
    if (touched) affected.add(pkg.name);
  }

  // A dependent of an affected package is affected too, however far the chain runs.
  let grew = true;
  while (grew) {
    grew = false;
    for (const pkg of workspace.packages) {
      if (!affected.has(pkg.name) && pkg.workspaceDeps.some((dep) => affected.has(dep))) {
        affected.add(pkg.name);
        grew = true;
      }
    }
  }
  return affected;
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
  const args = parseArgs(argv);
  const workspace = readWorkspace(root);
  if (!workspace.packages.some((pkg) => pkg.name === args.packageName)) {
    const known = workspace.packages.map((pkg) => pkg.name).join(", ");
    throw new Error(`web-affected: "${args.packageName}" is not a workspace package (known: ${known})`);
  }
  if (args.kind === "all") return "affected=true";
  const affected = affectedPackages(changedFilesBetween(args.base, args.head), workspace);
  return `affected=${affected.has(args.packageName)}`;
}

type CliArgs =
  | { readonly kind: "all"; readonly packageName: string }
  | { readonly kind: "diff"; readonly packageName: string; readonly base: string; readonly head: string };

function parseArgs(argv: readonly string[]): CliArgs {
  const values = new Map<string, string>();
  let all = false;
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (flag === "--all") {
      all = true;
    } else if (["--package", "--base", "--head"].includes(flag) && i + 1 < argv.length) {
      values.set(flag, argv[++i]);
    } else {
      throw new Error(`web-affected: unexpected argument "${flag}"`);
    }
  }

  const packageName = values.get("--package");
  if (packageName === undefined) throw new Error("web-affected: --package <name> is required");
  if (all) return { kind: "all", packageName };

  const base = values.get("--base");
  const head = values.get("--head");
  if (base === undefined || head === undefined) {
    throw new Error("web-affected: --base and --head are required unless --all is given");
  }
  return { kind: "diff", packageName, base, head };
}

function readMember(root: string, member: string): WorkspacePackage {
  const dir = member.replace(/^\.\//, "").replace(/\/+$/, "");
  if (dir.includes("*")) {
    throw new Error(`web-affected: workspace "${member}" is a glob; list each package directory`);
  }
  const manifestPath = join(root, dir, "package.json");
  const manifest = readManifest(manifestPath);
  if (typeof manifest.name !== "string") {
    throw new Error(`${manifestPath}: "name" must be a string`);
  }
  return { name: manifest.name, dir, workspaceDeps: workspaceDependencies(manifest) };
}

function workspaceDependencies(manifest: PackageManifest): string[] {
  const specs = [
    ...Object.entries(manifest.dependencies ?? {}),
    ...Object.entries(manifest.devDependencies ?? {}),
  ];
  const names = specs
    .filter(([, spec]) => spec.startsWith("workspace:"))
    .map(([name]) => name);
  return [...new Set(names)];
}

function readManifest(path: string): PackageManifest {
  return JSON.parse(readFileSync(path, "utf8")) as PackageManifest;
}

/** Repo-relative files changed between `base` and `head`, renames split into delete + add. */
function gitChangedFiles(root: string, base: string, head: string): string[] {
  const out = execFileSync(
    "git",
    ["diff", "--name-only", "--no-renames", `${base}...${head}`],
    { cwd: root, encoding: "utf8" },
  );
  return out.split("\n").filter((line) => line.length > 0);
}

if (import.meta.main) {
  const root = resolve(import.meta.dir, "..");
  const line = runCli(process.argv.slice(2), root, (base, head) => gitChangedFiles(root, base, head));
  process.stdout.write(`${line}\n`);
}
