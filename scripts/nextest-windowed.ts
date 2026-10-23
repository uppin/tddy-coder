/**
 * Run the workspace's unit leg in a sliding window, so its test binaries never all exist at once.
 *
 * Every `tests/*.rs` file, and every crate's own unit tests, link into a binary of their own that
 * statically carries the crate's whole dependency graph — about 0.1–0.2 GB each on the Linux runner,
 * even with line-table debug info. `cargo nextest run --workspace` builds all of them before it runs
 * one, and at roughly a thousand binaries that is more than the runner's disk (`Rust tests` died in
 * `rustc` with "No space left on device" with 110 GB free at the start). A filterset (`-E`) does not
 * help: nextest applies it after building.
 *
 * So the packages are taken in batches, capped by how many test binaries a batch builds. Each batch
 * builds, runs, and then deletes the test binaries it built — named by `cargo nextest list`, never
 * guessed — before the next one starts. The shared dependency artifacts stay, so a later batch
 * reuses them; the fixture binaries other suites exec (`Rust build`'s, under `target/debug/`) are
 * nextest's non-test binaries and are never deleted. Within a batch cargo and nextest stay as
 * parallel as ever: the window bounds disk, not concurrency.
 *
 * Usage: `bun scripts/nextest-windowed.ts (--batches <n> | --max-binaries <n>) [--packages a,b]
 * [-- <nextest run args>...]`, from the repo root inside the dev shell. `--batches` splits the
 * workspace into that many batches balanced by test binaries; `--max-binaries` caps each batch
 * instead. `--packages` limits the run to the named packages (one
 * batch's worth, to reproduce it locally). Each batch's JUnit report is kept as
 * `target/nextest/<profile>/junit-<batch>.xml`. Exits non-zero if any batch did.
 */

import { existsSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { testBinaries, workspacePackages } from "./nextest-serial-groups";

export interface WindowPackage {
  name: string;
  /** How many test binaries building this package's tests produces. */
  binaries: number;
}

/**
 * The packages in batches of at most `cap` test binaries, in workspace order. A package that alone
 * exceeds the cap is a batch of its own — it cannot be split, and skipping it would drop its tests.
 * A package with no test binaries builds nothing and is left out.
 */
export function windowBatches(packages: WindowPackage[], cap: number): string[][] {
  if (!(cap >= 1)) throw new Error(`the binary cap must be at least 1, got ${cap}`);
  const batches: string[][] = [];
  let current: string[] = [];
  let size = 0;
  for (const pkg of packages) {
    if (pkg.binaries <= 0) continue;
    if (current.length > 0 && size + pkg.binaries > cap) {
      batches.push(current);
      current = [];
      size = 0;
    }
    current.push(pkg.name);
    size += pkg.binaries;
  }
  if (current.length > 0) batches.push(current);
  return batches;
}

/**
 * The packages in exactly `count` contiguous batches (fewer only when there are fewer packages),
 * balanced by test binaries rather than by package count: a batch closes once the running total
 * reaches its share of the whole. Workspace order is kept, so a batch's packages build together the
 * way they did before.
 */
export function balancedBatches(packages: WindowPackage[], count: number): string[][] {
  if (!(count >= 1)) throw new Error(`the batch count must be at least 1, got ${count}`);
  const withTests = packages.filter((pkg) => pkg.binaries > 0);
  const total = withTests.reduce((sum, pkg) => sum + pkg.binaries, 0);
  const batches: string[][] = [];
  let current: string[] = [];
  let running = 0;
  for (const pkg of withTests) {
    current.push(pkg.name);
    running += pkg.binaries;
    const isLastBatch = batches.length === count - 1;
    if (!isLastBatch && running >= ((batches.length + 1) * total) / count) {
      batches.push(current);
      current = [];
    }
  }
  if (current.length > 0) batches.push(current);
  return batches;
}

/**
 * The test binaries in a `cargo nextest list --list-type binaries-only --message-format json`
 * listing. The dev shell prints a banner line on stdout first, so the JSON starts at the first `{`.
 * Only `rust-binaries` are test binaries; `rust-build-meta.non-test-binaries` are fixtures.
 */
export function testBinaryPaths(listing: string): string[] {
  const start = listing.indexOf("{");
  if (start < 0) throw new Error(`no JSON in the nextest listing:\n${listing.slice(0, 500)}`);
  const parsed = JSON.parse(listing.slice(start)) as {
    "rust-binaries"?: Record<string, { "binary-path": string }>;
  };
  return Object.values(parsed["rust-binaries"] ?? {}).map((binary) => binary["binary-path"]);
}

function packageArgs(batch: string[]): string[] {
  return batch.flatMap((name) => ["-p", name]);
}

function group<T>(title: string, body: () => T): T {
  console.log(`::group::${title}`);
  try {
    return body();
  } finally {
    console.log("::endgroup::");
  }
}

function parseArgs(argv: string[]): {
  split: { batches: number } | { cap: number };
  only: string[] | null;
  profile: string;
  nextestArgs: string[];
} {
  const separator = argv.indexOf("--");
  const own = separator < 0 ? argv : argv.slice(0, separator);
  const nextestArgs = separator < 0 ? [] : argv.slice(separator + 1);
  const capAt = own.indexOf("--max-binaries");
  const batchesAt = own.indexOf("--batches");
  const usage = "usage: nextest-windowed.ts (--batches <n> | --max-binaries <n>) [--packages a,b] [-- <nextest run args>...]";
  if ((capAt < 0) === (batchesAt < 0)) throw new Error(usage);
  const value = own[(capAt >= 0 ? capAt : batchesAt) + 1];
  if (value === undefined) throw new Error(usage);
  const split = capAt >= 0 ? { cap: Number(value) } : { batches: Number(value) };
  const onlyAt = own.indexOf("--packages");
  const only = onlyAt >= 0 ? (own[onlyAt + 1] ?? "").split(",").filter(Boolean) : null;
  const profileAt = nextestArgs.indexOf("--profile");
  const profile = profileAt >= 0 ? nextestArgs[profileAt + 1] : "default";
  return { split, only, profile, nextestArgs };
}

function main(): number {
  const { split, only, profile, nextestArgs } = parseArgs(process.argv.slice(2));
  const repoRoot = process.cwd();
  const targets = testBinaries(repoRoot);
  // One binary per integration-test target, plus the package's own unit-test binary.
  const members = workspacePackages(repoRoot);
  const unknown = (only ?? []).filter((name) => !members.some((pkg) => pkg.name === name));
  if (unknown.length > 0) throw new Error(`not workspace packages: ${unknown.join(", ")}`);
  const packages = members.filter((pkg) => only === null || only.includes(pkg.name)).map((pkg) => ({
    name: pkg.name,
    binaries: targets.filter((target) => target.package === pkg.name).length + 1,
  }));
  const batches = "cap" in split ? windowBatches(packages, split.cap) : balancedBatches(packages, split.batches);
  const reportDir = join(repoRoot, "target", "nextest", profile);
  console.log(
    `${packages.length} packages in ${batches.length} batches: ${batches
      .map((batch) => batch.reduce((sum, name) => sum + (packages.find((pkg) => pkg.name === name)?.binaries ?? 0), 0))
      .join(" + ")} test binaries`,
  );

  const failed: number[] = [];
  batches.forEach((batch, index) => {
    const n = index + 1;
    const label = `batch ${n}/${batches.length}: ${batch.join(", ")}`;
    const run = group(`run ${label}`, () =>
      spawnSync("cargo", ["nextest", "run", "--no-tests=pass", ...packageArgs(batch), ...nextestArgs], {
        stdio: "inherit",
      }),
    );
    if (run.status !== 0) failed.push(n);

    const report = join(reportDir, "junit.xml");
    if (existsSync(report)) renameSync(report, join(reportDir, `junit-${n}.xml`));

    group(`delete the test binaries of batch ${n}`, () => {
      const listing = spawnSync(
        "cargo",
        ["nextest", "list", "--locked", "--list-type", "binaries-only", "--message-format", "json", ...packageArgs(batch)],
        { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
      );
      if (listing.status !== 0) {
        // The batch did not build, so it left no binaries to reclaim; its run already failed.
        console.log(`listing failed (exit ${listing.status}); nothing to delete\n${listing.stderr}`);
        return;
      }
      const paths = testBinaryPaths(listing.stdout);
      for (const path of paths) rmSync(path, { force: true });
      console.log(`deleted ${paths.length} test binaries`);
      spawnSync("df", ["-h", "/"], { stdio: "inherit" });
    });
  });

  if (failed.length > 0) {
    console.log(`::error::batches that failed: ${failed.join(", ")} of ${batches.length}`);
    return 1;
  }
  return 0;
}

if (import.meta.main) process.exit(main());
