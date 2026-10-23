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
 * Usage: `bun scripts/nextest-windowed.ts --max-binaries <n> [--packages a,b] [-- <nextest run args>...]`,
 * from the repo root inside the dev shell. `--packages` limits the run to the named packages (one
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
  cap: number;
  only: string[] | null;
  profile: string;
  nextestArgs: string[];
} {
  const split = argv.indexOf("--");
  const own = split < 0 ? argv : argv.slice(0, split);
  const nextestArgs = split < 0 ? [] : argv.slice(split + 1);
  const capAt = own.indexOf("--max-binaries");
  if (capAt < 0 || own[capAt + 1] === undefined) {
    throw new Error("usage: nextest-windowed.ts --max-binaries <n> [-- <nextest run args>...]");
  }
  const onlyAt = own.indexOf("--packages");
  const only = onlyAt >= 0 ? (own[onlyAt + 1] ?? "").split(",").filter(Boolean) : null;
  const profileAt = nextestArgs.indexOf("--profile");
  const profile = profileAt >= 0 ? nextestArgs[profileAt + 1] : "default";
  return { cap: Number(own[capAt + 1]), only, profile, nextestArgs };
}

function main(): number {
  const { cap, only, profile, nextestArgs } = parseArgs(process.argv.slice(2));
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
  const batches = windowBatches(packages, cap);
  const reportDir = join(repoRoot, "target", "nextest", profile);
  console.log(`${packages.length} packages in ${batches.length} batches of at most ${cap} test binaries`);

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
