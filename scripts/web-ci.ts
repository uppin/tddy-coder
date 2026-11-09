/**
 * The work behind `.github/workflows/web.yml`, discovered from the root `workspaces` list so a package
 * added later needs no CI edit.
 *
 * A package takes part through its own `package.json`:
 *
 *   scripts.build:ci              built by `Web build`
 *   scripts.typecheck             run by `Web lint`; a package without one, or whose typecheck
 *                                 fails today, is on scripts/web-lint-exclusions.json with the
 *                                 todo record that removes it
 *   scripts.test:ci               its unit suites, run by one `Web tests` shard
 *   scripts.cypress:component:ci  its Cypress component specs, split across the shards
 *   tddyCi.needs                  what a shard must prepare before running its tests:
 *                                 "web-bundle", "tddy-coder", "echo-server", "livekit-image"
 *
 * CLI:
 *
 *   bun scripts/web-ci.ts plan --shard <i> --of <n> (--all | --base <sha> --head <sha>) --out <file>
 *       writes the shard's plan as JSON and prints `key=value` lines for $GITHUB_OUTPUT:
 *       `affected`, `cypress`, and one `needs-<need>` per known need
 *   bun scripts/web-ci.ts run --plan <file>      runs the plan's suites, every one even after a failure
 *   bun scripts/web-ci.ts build                  runs every package's build:ci
 *   bun scripts/web-ci.ts lint                   runs every package's typecheck
 */

import { execFileSync, spawnSync } from "node:child_process";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { affectedPackages, readWorkspace, type WorkspacePackage } from "./web-affected";

export const KNOWN_NEEDS = ["web-bundle", "tddy-coder", "echo-server", "livekit-image"] as const;
export type Need = (typeof KNOWN_NEEDS)[number];

/** A workspace package with the CI facts its `package.json` declares. */
export interface CiPackage {
  readonly name: string;
  readonly dir: string;
  readonly scripts: ReadonlySet<string>;
  readonly needs: readonly Need[];
  /** Cypress component spec paths relative to the package directory, sorted. */
  readonly componentSpecs: readonly string[];
}

export interface ShardPlan {
  readonly shard: number;
  readonly of: number;
  /** Packages whose `test:ci` this shard runs. */
  readonly unit: readonly string[];
  /** Package → the component specs this shard runs. */
  readonly component: Readonly<Record<string, readonly string[]>>;
  readonly needs: readonly Need[];
}

export interface LintExclusion {
  readonly package: string;
  readonly reason: string;
  /** The `docs/dev/todo/` record that removes the exclusion, or null when there is nothing to typecheck. */
  readonly todo: string | null;
}

export function readCiPackages(root: string): CiPackage[] {
  return readWorkspace(root).packages.map((pkg) => readCiPackage(root, pkg));
}

function readCiPackage(root: string, pkg: WorkspacePackage): CiPackage {
  const manifestPath = join(root, pkg.dir, "package.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8")) as {
    scripts?: Record<string, string>;
    tddyCi?: { needs?: unknown };
  };
  const needs = manifest.tddyCi?.needs ?? [];
  if (!Array.isArray(needs) || !needs.every((n) => (KNOWN_NEEDS as readonly string[]).includes(n))) {
    throw new Error(`${manifestPath}: tddyCi.needs must list only ${KNOWN_NEEDS.join(", ")}`);
  }
  const scripts = new Set(Object.keys(manifest.scripts ?? {}));
  const componentSpecs = scripts.has("cypress:component:ci")
    ? findSpecs(join(root, pkg.dir), "cypress/component")
    : [];
  return { name: pkg.name, dir: pkg.dir, scripts, needs: needs as Need[], componentSpecs };
}

function findSpecs(packageDir: string, sub: string): string[] {
  const start = join(packageDir, sub);
  const found: string[] = [];
  const walk = (dir: string) => {
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      return;
    }
    for (const entry of entries) {
      const path = join(dir, entry);
      if (statSync(path).isDirectory()) walk(path);
      else if (/\.cy\.tsx?$/.test(entry)) found.push(relative(packageDir, path));
    }
  };
  walk(start);
  return found.sort();
}

/**
 * The share of `affected` packages' suites that shard `shard` (1-based) of `of` runs. Unit suites go
 * whole, by the package's index in the sorted affected list; component specs are pooled across every
 * affected package, sorted, and dealt by index — so the split is deterministic and roughly even.
 */
export function planShard(
  packages: readonly CiPackage[],
  affected: ReadonlySet<string>,
  shard: number,
  of: number,
): ShardPlan {
  if (!Number.isInteger(of) || of < 1 || !Number.isInteger(shard) || shard < 1 || shard > of) {
    throw new Error(`web-ci: shard ${shard} of ${of} is not a valid shard`);
  }
  const mine = (index: number) => index % of === shard - 1;
  const chosen = packages.filter((p) => affected.has(p.name)).sort((a, b) => a.name.localeCompare(b.name));

  const unit = chosen.filter((p) => p.scripts.has("test:ci")).map((p) => p.name).filter((_, i) => mine(i));

  const pool = chosen.flatMap((p) => p.componentSpecs.map((spec) => ({ pkg: p.name, spec })));
  const component: Record<string, string[]> = {};
  pool.forEach(({ pkg, spec }, i) => {
    if (mine(i)) (component[pkg] ??= []).push(spec);
  });

  const running = new Set([...unit, ...Object.keys(component)]);
  const needs = KNOWN_NEEDS.filter((need) =>
    chosen.some((p) => running.has(p.name) && p.needs.includes(need)),
  );
  return { shard, of, unit, component, needs };
}

/** The `$GITHUB_OUTPUT` lines for a plan. */
export function planOutputs(plan: ShardPlan): string[] {
  const affected = plan.unit.length > 0 || Object.keys(plan.component).length > 0;
  return [
    `affected=${affected}`,
    `cypress=${Object.keys(plan.component).length > 0}`,
    ...KNOWN_NEEDS.map((need) => `needs-${need}=${plan.needs.includes(need)}`),
  ];
}

/**
 * Packages without a `typecheck` script that the exclusion list does not account for, and entries
 * naming a package the workspace no longer has. A listed package is skipped even when it has a
 * `typecheck` script — that is how a package whose script fails today stays out of the gate.
 */
export function lintGaps(
  packages: readonly CiPackage[],
  exclusions: readonly LintExclusion[],
): { unlisted: string[]; stale: string[] } {
  const excluded = new Set(exclusions.map((e) => e.package));
  const names = new Set(packages.map((p) => p.name));
  const unlisted = packages.filter((p) => !p.scripts.has("typecheck") && !excluded.has(p.name)).map((p) => p.name);
  const stale = exclusions.filter((e) => !names.has(e.package)).map((e) => e.package);
  return { unlisted, stale };
}

export function readLintExclusions(root: string): LintExclusion[] {
  return JSON.parse(readFileSync(join(root, "scripts", "web-lint-exclusions.json"), "utf8")) as LintExclusion[];
}

function runAll(root: string, label: string, commands: { name: string; cwd: string; args: string[] }[]): number {
  const failed: string[] = [];
  for (const { name, cwd, args } of commands) {
    process.stderr.write(`\n::group::${label} ${name}: bun ${args.join(" ")}\n`);
    const result = spawnSync("bun", args, { cwd: join(root, cwd), stdio: "inherit" });
    process.stderr.write("::endgroup::\n");
    if (result.status !== 0) failed.push(name);
  }
  if (failed.length > 0) {
    process.stderr.write(`::error::${label} failed for: ${failed.join(", ")}\n`);
    return 1;
  }
  return 0;
}

function changedFiles(root: string, base: string, head: string): string[] {
  return execFileSync("git", ["diff", "--name-only", "--no-renames", `${base}...${head}`], { cwd: root, encoding: "utf8" })
    .split("\n")
    .filter((line) => line.length > 0);
}

function flags(argv: readonly string[]): Map<string, string | true> {
  const out = new Map<string, string | true>();
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    if (!flag.startsWith("--")) throw new Error(`web-ci: unexpected argument "${flag}"`);
    if (flag === "--all") out.set(flag, true);
    else if (i + 1 < argv.length) out.set(flag, argv[++i]);
    else throw new Error(`web-ci: ${flag} needs a value`);
  }
  return out;
}

export function main(argv: readonly string[], root: string): number {
  const [command, ...rest] = argv;
  const args = flags(rest);
  const packages = readCiPackages(root);

  if (command === "plan") {
    const shard = Number(args.get("--shard"));
    const of = Number(args.get("--of"));
    const out = args.get("--out");
    if (typeof out !== "string") throw new Error("web-ci: plan needs --out <file>");
    let affected: Set<string>;
    if (args.get("--all") === true) affected = new Set(packages.map((p) => p.name));
    else {
      const base = args.get("--base");
      const head = args.get("--head");
      if (typeof base !== "string" || typeof head !== "string") {
        throw new Error("web-ci: plan needs --base and --head unless --all is given");
      }
      affected = affectedPackages(changedFiles(root, base, head), readWorkspace(root));
    }
    const plan = planShard(packages, affected, shard, of);
    writeFileSync(out, JSON.stringify(plan, null, 2));
    process.stdout.write(`${planOutputs(plan).join("\n")}\n`);
    return 0;
  }

  if (command === "run") {
    const planPath = args.get("--plan");
    if (typeof planPath !== "string") throw new Error("web-ci: run needs --plan <file>");
    const plan = JSON.parse(readFileSync(planPath, "utf8")) as ShardPlan;
    const dirOf = (name: string) => packages.find((p) => p.name === name)?.dir ?? `packages/${name}`;
    return runAll(root, `shard ${plan.shard}/${plan.of}`, [
      ...plan.unit.map((name) => ({ name: `${name} unit`, cwd: dirOf(name), args: ["run", "test:ci"] })),
      ...Object.entries(plan.component).map(([name, specs]) => ({
        name: `${name} component (${specs.length} specs)`,
        cwd: dirOf(name),
        args: ["run", "cypress:component:ci", "--spec", specs.join(",")],
      })),
    ]);
  }

  if (command === "build") {
    return runAll(
      root,
      "build",
      packages.filter((p) => p.scripts.has("build:ci")).map((p) => ({ name: p.name, cwd: p.dir, args: ["run", "build:ci"] })),
    );
  }

  if (command === "lint") {
    const { unlisted, stale } = lintGaps(packages, readLintExclusions(root));
    if (unlisted.length > 0 || stale.length > 0) {
      process.stderr.write(
        `::error::scripts/web-lint-exclusions.json is out of date — without a typecheck script and not listed: ` +
          `[${unlisted.join(", ")}]; listed but not a workspace package: [${stale.join(", ")}]\n`,
      );
      return 1;
    }
    const excluded = new Set(readLintExclusions(root).map((e) => e.package));
    return runAll(
      root,
      "typecheck",
      packages.filter((p) => p.scripts.has("typecheck") && !excluded.has(p.name)).map((p) => ({ name: p.name, cwd: p.dir, args: ["run", "typecheck"] })),
    );
  }

  throw new Error(`web-ci: unknown command "${command ?? ""}" (plan | run | build | lint)`);
}

if (import.meta.main) {
  process.exit(main(process.argv.slice(2), resolve(import.meta.dir, "..")));
}
