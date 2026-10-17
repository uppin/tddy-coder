/**
 * The `cargo nextest archive` target arguments for the e2e set.
 *
 * The e2e leg archives only the test targets `.config/rust-e2e.filterset` selects (an archive of the
 * whole workspace is tens of GB and does not fit a runner). The arguments are derived from the
 * filterset, so the set is still written in one place, and the derivation refuses rather than
 * guesses:
 *
 * - an empty selection is an error, not an archive of everything;
 * - a `binary()` or `package()` the filterset names that matches no test target is an error, so a
 *   renamed or deleted test cannot silently shrink the leg;
 * - a filterset that can select a lib or bin unit-test binary is an error, because `--test` never
 *   builds one and the leg would silently skip it — while the other leg, which runs the complement,
 *   would skip it too.
 *
 * Usage: `bun scripts/nextest-e2e-archive.ts <filterset> <out-file>` writes the arguments, space
 * separated, to `<out-file>`. A file, not stdout: the dev shell prints a banner on stdout.
 */

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type FilterSubject, filterReferences, matches, parseFilterset } from "./nextest-filterset";
import { type TestTarget, testBinaries, workspacePackages } from "./nextest-serial-groups";

export interface E2eArchivePlan {
  /** Packages that own a selected test target, sorted. */
  packages: string[];
  /** Names of the selected integration-test targets, sorted and unique. */
  tests: string[];
  /** `-p <package>`... then `--test <name>`..., ready for `cargo nextest archive`. */
  args: string[];
}

/**
 * The archive arguments for `filterset`.
 *
 * `targets` is every integration-test target of the workspace; `unitBinaries` every lib and bin
 * unit-test binary, marked `unit`. Packages are named with `-p` so a test that exists in several
 * packages is built only where the filterset selects it. A package named with `-p` that lacks one of
 * the `--test` names builds the others (checked against cargo).
 */
export function e2eArchivePlan(filterset: string, targets: TestTarget[], unitBinaries: FilterSubject[]): E2eArchivePlan {
  const filter = parseFilterset(filterset);

  for (const reference of filterReferences(filter)) {
    const found =
      reference.kind === "package"
        ? targets.some((target) => target.package === reference.package)
        : targets.some(
            (target) =>
              target.binary === reference.binary &&
              (reference.package === undefined || target.package === reference.package),
          );
    if (!found) {
      const named =
        reference.kind === "package"
          ? `package(${reference.package})`
          : `${reference.package === undefined ? "" : `package(${reference.package}) and `}binary(${reference.binary})`;
      throw new Error(`the e2e filterset names ${named}, which matches no integration-test target in the workspace`);
    }
  }

  const unit = unitBinaries.filter((binary) => matches(filter, binary));
  if (unit.length > 0) {
    const names = unit.map((binary) => `${binary.package}::${binary.binary}`).join(", ");
    throw new Error(
      `the e2e filterset can select unit-test binaries (${names}); archiving only --test targets would drop them. ` +
        "Add kind(test) to the package() term or name the binaries.",
    );
  }

  const selected = targets.filter((target) => matches(filter, target));
  if (selected.length === 0) throw new Error("the e2e filterset selects no integration-test target");

  const packages = [...new Set(selected.map((target) => target.package))].sort();
  const tests = [...new Set(selected.map((target) => target.binary))].sort();
  return { packages, tests, args: [...packages.flatMap((pkg) => ["-p", pkg]), ...tests.flatMap((name) => ["--test", name])] };
}

/** Every lib and bin unit-test binary of the workspace rooted at `repoRoot`, by nextest's binary name. */
export function unitTestBinaries(repoRoot: string): FilterSubject[] {
  return workspacePackages(repoRoot).flatMap(({ name, dir }) => {
    const manifest = Bun.TOML.parse(readFileSync(join(dir, "Cargo.toml"), "utf8")) as {
      bin?: { name?: string }[];
    };
    const bins = new Set<string>((manifest.bin ?? []).flatMap((bin) => (bin.name === undefined ? [] : [bin.name])));
    if (existsSync(join(dir, "src", "main.rs"))) bins.add(name);
    const binDir = join(dir, "src", "bin");
    if (existsSync(binDir)) {
      for (const entry of readdirSync(binDir)) bins.add(entry.replace(/\.rs$/, ""));
    }
    return [name, ...bins].map((binary) => ({ package: name, binary, unit: true as const }));
  });
}

if (import.meta.main) {
  const [filtersetPath, outPath] = process.argv.slice(2);
  if (filtersetPath === undefined || outPath === undefined) {
    console.error("usage: nextest-e2e-archive.ts <filterset> <out-file>");
    process.exit(2);
  }
  const repoRoot = process.cwd();
  const plan = e2eArchivePlan(readFileSync(filtersetPath, "utf8"), testBinaries(repoRoot), unitTestBinaries(repoRoot));
  writeFileSync(outPath, `${plan.args.join(" ")}\n`);
  console.error(`e2e archive: ${plan.tests.length} test targets in ${plan.packages.length} packages`);
}
