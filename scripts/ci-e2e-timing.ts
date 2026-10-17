/**
 * Where the e2e leg's time goes.
 *
 * Three readings, all from files CI already produces:
 *
 * - `junit <junit.xml> <nextest.toml>` — one shard's run phase: wall time, per-binary run time, the
 *   slowest tests and the serial `rust-analyzer` group against the wall time, for the shard's step
 *   summary.
 * - `merge <archive-seconds> <nextest.toml> <junit.xml>...` — the same across every shard, with the
 *   archive build as its own phase, for the `Rust e2e tests` aggregator's step summary.
 * - `compile-share <cargo-timing.html> <rust-e2e.filterset>` — how much of a `cargo test --no-run
 *   --timings` build went to e2e test targets, to the other test targets, and to everything else.
 *   It decides whether splitting the compile by test target is worth the manifest churn.
 *
 * Parsers take file *contents*, so they need no cargo and no repository.
 */

import { filterPredicate, filterReferences, matches, parseFilterset } from "./nextest-filterset";
import { ciSerialisation, type CiSerialisation } from "./nextest-serial-groups";

/** The nextest test-group whose serial sum is the lower bound on a shard's run time. */
export const SERIAL_GROUP = "rust-analyzer";

/** How many of the slowest tests a summary lists. */
export const SLOWEST_TESTS = 20;

/** Share of a leg's compile that the leg would skip, at or above which the split is worth doing. */
export const SPLIT_WORTH_IT_PERCENT = 25;

export interface BinaryTiming {
  binary: string;
  seconds: number;
  tests: number;
}

export interface CompileShare {
  e2eTestSeconds: number;
  otherTestSeconds: number;
  otherSeconds: number;
  totalSeconds: number;
  /** What the e2e leg would stop compiling: every test target that is not e2e, as % of total. */
  e2eLegSkippablePercent: number;
  /** What the unit leg would stop compiling: every e2e test target, as % of total. */
  unitLegSkippablePercent: number;
  /** Filterset binaries that never appear as a test target in the report. */
  missingFromReport: string[];
}

export type Verdict = "proceed" | "stop";

export interface JunitCase {
  /** nextest's binary id: `pkg`, `pkg::test_name` or `pkg::bin/name`. */
  suite: string;
  package: string;
  binary: string;
  name: string;
  seconds: number;
}

export interface JunitRun {
  /** The run phase's wall time: the `time` of the report's root element. */
  wallSeconds: number;
  cases: JunitCase[];
}

/** A nextest JUnit report: the run phase's wall time and every test case. */
export function parseJunit(junitXml: string): JunitRun {
  const root = /<testsuites\s[^>]*?\btime="([\d.eE+-]+)"/.exec(junitXml);
  if (root === null) throw new Error("no <testsuites time=...> in the JUnit report");
  const cases: JunitCase[] = [];
  // A self-closing `<testsuite .../>` holds no cases; `[^/>]>` keeps it from swallowing the next suite.
  const suite = /<testsuite\s[^>]*?\bname="([^"]*)"[^>]*[^/>]>([\s\S]*?)<\/testsuite>/g;
  for (const [, suiteName, inner] of junitXml.matchAll(suite)) {
    for (const [, name, time] of inner.matchAll(/<testcase\b[^>]*?\bname="([^"]*)"[^>]*?\btime="([\d.eE+-]+)"/g)) {
      cases.push({
        suite: suiteName,
        package: packageOfSuite(suiteName),
        binary: binaryOfSuite(suiteName),
        name,
        seconds: Number(time),
      });
    }
  }
  return { wallSeconds: Number(root[1]), cases };
}

/** Per-binary run time, slowest first. */
export function perBinaryTimings(junitXml: string): BinaryTiming[] {
  return binaryTimings(parseJunit(junitXml).cases);
}

/** Per-binary run time of `cases`, slowest first. */
export function binaryTimings(cases: JunitCase[]): BinaryTiming[] {
  const byBinary = new Map<string, BinaryTiming>();
  for (const testCase of cases) {
    const row = byBinary.get(testCase.binary) ?? { binary: testCase.binary, seconds: 0, tests: 0 };
    row.seconds += testCase.seconds;
    row.tests += 1;
    byBinary.set(testCase.binary, row);
  }
  return [...byBinary.values()].sort((a, b) => b.seconds - a.seconds || a.binary.localeCompare(b.binary));
}

/** The `limit` slowest test cases, slowest first. */
export function slowestTests(cases: JunitCase[], limit = SLOWEST_TESTS): JunitCase[] {
  return [...cases]
    .sort((a, b) => b.seconds - a.seconds || `${a.suite}${a.name}`.localeCompare(`${b.suite}${b.name}`))
    .slice(0, limit);
}

/** Summed run time and count of the cases that nextest runs in test-group `group` under the `ci` profile. */
export function groupSerialSum(
  cases: JunitCase[],
  config: CiSerialisation,
  group = SERIAL_GROUP,
): { seconds: number; tests: number } {
  const inGroup = cases.filter((testCase) => testGroupOf(config, testCase) === group);
  return { seconds: inGroup.reduce((sum, testCase) => sum + testCase.seconds, 0), tests: inGroup.length };
}

/** The test-group nextest puts `subject` in: the first override that selects it and sets one. */
function testGroupOf(config: CiSerialisation, subject: { package: string; binary: string }): string | undefined {
  return config.overrides.find((override) => matches(override.filter, subject))?.testGroup;
}

/** The package of a nextest binary id: everything before the first `::`, or the whole id for a lib. */
function packageOfSuite(suiteName: string): string {
  const separator = suiteName.indexOf("::");
  return separator < 0 ? suiteName : suiteName.slice(0, separator);
}

/** nextest names a suite by binary id: `pkg`, `pkg::test_name` or `pkg::bin/name`. */
function binaryOfSuite(suiteName: string): string {
  const afterPackage = suiteName.slice(suiteName.lastIndexOf("::") + 2);
  return afterPackage.replace(/^bin\//, "");
}

interface TestUnit {
  pkg: string;
  binary: string;
  seconds: number;
}

/** Compile time split into e2e test targets, other test targets and the rest. */
export function compileShare(timingsHtml: string, filterset: string): CompileShare {
  const matches = filterPredicate(filterset);
  let e2eTestSeconds = 0;
  let otherTestSeconds = 0;
  let otherSeconds = 0;
  const testBinaries = new Set<string>();

  for (const unit of readUnits(timingsHtml)) {
    // Cargo marks a test unit by its target, not its `mode` (which is `todo` for every compile):
    // ` test "name" (test)` for an integration target, ` lib (test)` / ` bin "name" (test)` for
    // unit tests inside a package.
    if (!/\(test\)\s*$/.test(unit.target)) {
      otherSeconds += unit.duration;
      continue;
    }
    // A lib's unit tests run as their package; nextest names that binary by the package.
    const binary = /^\s*(?:test|bin) "([^"]+)"/.exec(unit.target)?.[1] ?? unit.name;
    testBinaries.add(binary);
    if (matches({ package: unit.name, binary })) e2eTestSeconds += unit.duration;
    else otherTestSeconds += unit.duration;
  }

  const totalSeconds = e2eTestSeconds + otherTestSeconds + otherSeconds;
  const percentOfTotal = (seconds: number) => (totalSeconds === 0 ? 0 : (seconds * 100) / totalSeconds);
  return {
    e2eTestSeconds,
    otherTestSeconds,
    otherSeconds,
    totalSeconds,
    e2eLegSkippablePercent: percentOfTotal(otherTestSeconds),
    unitLegSkippablePercent: percentOfTotal(e2eTestSeconds),
    missingFromReport: filtersetBinaries(filterset).filter((binary) => !testBinaries.has(binary)),
  };
}

interface ReportUnit {
  name: string;
  target: string;
  duration: number;
}

/** The `UNIT_DATA` array cargo embeds in `cargo-timing.html`. */
function readUnits(timingsHtml: string): ReportUnit[] {
  const start = timingsHtml.indexOf("UNIT_DATA");
  const open = start < 0 ? -1 : timingsHtml.indexOf("[", start);
  if (open < 0) throw new Error("no UNIT_DATA array in the timings report");
  // The array is JSON; find its end by bracket depth, skipping brackets inside strings.
  let depth = 0;
  let inString = false;
  for (let i = open; i < timingsHtml.length; i++) {
    const c = timingsHtml[i];
    if (inString) {
      if (c === "\\") i++;
      else if (c === '"') inString = false;
    } else if (c === '"') inString = true;
    else if (c === "[") depth++;
    else if (c === "]" && --depth === 0) return JSON.parse(timingsHtml.slice(open, i + 1));
  }
  throw new Error("unterminated UNIT_DATA array in the timings report");
}

/** Binaries a filterset names with `binary(...)`, in order of appearance. */
function filtersetBinaries(filterset: string): string[] {
  return filterReferences(parseFilterset(filterset)).flatMap((ref) => (ref.kind === "binary" ? [ref.binary] : []));
}

/** `proceed` when the leg would skip at least {@link SPLIT_WORTH_IT_PERCENT}% of its compile. */
export function verdict(skippablePercent: number): Verdict {
  return skippablePercent >= SPLIT_WORTH_IT_PERCENT ? "proceed" : "stop";
}

const seconds1 = (seconds: number) => seconds.toFixed(1);

function binaryTable(cases: JunitCase[]): string[] {
  const rows = binaryTimings(cases);
  const lines = ["| Binary | Seconds | Tests |", "|---|---:|---:|"];
  for (const row of rows) lines.push(`| ${row.binary} | ${seconds1(row.seconds)} | ${row.tests} |`);
  const total = rows.reduce((sum, row) => sum + row.seconds, 0);
  lines.push(`| **total** | **${seconds1(total)}** | **${cases.length}** |`);
  return lines;
}

function slowestTable(cases: JunitCase[]): string[] {
  const lines = ["| Test | Binary | Seconds |", "|---|---|---:|"];
  for (const testCase of slowestTests(cases)) {
    lines.push(`| ${testCase.name} | ${testCase.binary} | ${seconds1(testCase.seconds)} |`);
  }
  return lines;
}

/** How the serial group's summed time compares with the run's wall time, as one line. */
function serialGroupLine(run: JunitRun, config: CiSerialisation): string {
  const group = groupSerialSum(run.cases, config);
  const share = run.wallSeconds === 0 ? 0 : (group.seconds * 100) / run.wallSeconds;
  return (
    `\`${SERIAL_GROUP}\` group (one test at a time): ${group.tests} tests, serial sum ` +
    `${seconds1(group.seconds)} s = ${share.toFixed(0)}% of the run's ${seconds1(run.wallSeconds)} s wall time`
  );
}

/** One shard's run phase, as markdown for its step summary. */
export function shardSummary(run: JunitRun, config: CiSerialisation): string {
  return [
    "### e2e run phase",
    "",
    `- run wall time: ${seconds1(run.wallSeconds)} s (${run.cases.length} tests)`,
    `- ${serialGroupLine(run, config)}`,
    "",
    "### e2e run time per test binary",
    "",
    ...binaryTable(run.cases),
    "",
    `### ${SLOWEST_TESTS} slowest tests`,
    "",
    ...slowestTable(run.cases),
  ].join("\n");
}

export interface ShardRun {
  label: string;
  run: JunitRun;
}

/** Every shard of the e2e leg, as markdown for the aggregator's step summary. */
export function mergedSummary(archiveSeconds: number, shards: ShardRun[], config: CiSerialisation): string {
  const cases = shards.flatMap((shard) => shard.run.cases);
  const slowest = shards.reduce((max, shard) => Math.max(max, shard.run.wallSeconds), 0);
  const lines = [
    "### e2e phases",
    "",
    `- test archive build (compile, once for every leg): ${seconds1(archiveSeconds)} s`,
    `- run phase: ${seconds1(slowest)} s — the slowest of ${shards.length} shards`,
    "",
    "| Shard | Tests | Run wall s | Serial group tests | Serial group sum s | Group sum / wall |",
    "|---|---:|---:|---:|---:|---:|",
  ];
  for (const { label, run } of shards) {
    const group = groupSerialSum(run.cases, config);
    const share = run.wallSeconds === 0 ? 0 : (group.seconds * 100) / run.wallSeconds;
    lines.push(
      `| ${label} | ${run.cases.length} | ${seconds1(run.wallSeconds)} | ${group.tests} | ` +
        `${seconds1(group.seconds)} | ${share.toFixed(0)}% |`,
    );
  }
  const total = groupSerialSum(cases, config);
  lines.push(
    "",
    `\`${SERIAL_GROUP}\` group across all shards: ${total.tests} tests, ${seconds1(total.seconds)} s summed — ` +
      `what one shard would take serially.`,
    "",
    "### e2e run time per test binary (all shards)",
    "",
    ...binaryTable(cases),
    "",
    `### ${SLOWEST_TESTS} slowest tests (all shards)`,
    "",
    ...slowestTable(cases),
  );
  return lines.join("\n");
}

function compileShareSummary(share: CompileShare): string {
  const decision = verdict(share.e2eLegSkippablePercent);
  const lines = [
    "### Compile time by target kind",
    "",
    `- e2e test targets: ${share.e2eTestSeconds.toFixed(0)} s`,
    `- other test targets: ${share.otherTestSeconds.toFixed(0)} s`,
    `- everything else: ${share.otherSeconds.toFixed(0)} s`,
    `- total: ${share.totalSeconds.toFixed(0)} s`,
    "",
    `The e2e leg would skip ${share.e2eLegSkippablePercent.toFixed(1)}% of its compile ` +
      `(the unit leg ${share.unitLegSkippablePercent.toFixed(1)}%). ` +
      `Threshold ${SPLIT_WORTH_IT_PERCENT}%: **${decision}**.`,
  ];
  if (share.missingFromReport.length > 0) {
    lines.push("", `Filterset binaries absent from the report: ${share.missingFromReport.join(", ")}`);
  }
  return lines.join("\n");
}

if (import.meta.main) {
  const [command, ...args] = process.argv.slice(2);
  const read = (path: string) => Bun.file(path).text();
  if (command === "junit" && args.length === 2) {
    console.log(shardSummary(parseJunit(await read(args[0])), ciSerialisation(await read(args[1]))));
  } else if (command === "merge" && args.length >= 3) {
    const [archive, nextestToml, ...reports] = args;
    const archiveSeconds = Number(archive);
    if (!Number.isFinite(archiveSeconds)) throw new Error(`archive seconds is not a number: ${archive}`);
    const shards = await Promise.all(
      reports.map(async (path) => ({ label: shardLabel(path), run: parseJunit(await read(path)) })),
    );
    console.log(mergedSummary(archiveSeconds, shards, ciSerialisation(await read(nextestToml))));
  } else if (command === "compile-share" && args.length === 2) {
    console.log(compileShareSummary(compileShare(await read(args[0]), await read(args[1]))));
  } else {
    console.error(
      "usage: ci-e2e-timing.ts junit <junit.xml> <nextest.toml> | " +
        "merge <archive-seconds> <nextest.toml> <junit.xml>... | " +
        "compile-share <cargo-timing.html> <filterset>",
    );
    process.exit(2);
  }
}

/** A shard's name from where its report was downloaded: `.../junit-rust-e2e-shard-3/junit.xml` is `shard 3`. */
export function shardLabel(reportPath: string): string {
  const directory = reportPath.split("/").slice(-2, -1)[0] ?? reportPath;
  return /shard-(\d+)$/.exec(directory)?.[0].replace("-", " ") ?? directory;
}
