/**
 * Where the e2e leg's time goes.
 *
 * Two readings, both from files CI already produces:
 *
 * - `junit <junit.xml>` — per-binary run time of the e2e leg, for the check's step summary.
 * - `compile-share <cargo-timing.html> <rust-e2e.filterset>` — how much of a `cargo test --no-run
 *   --timings` build went to e2e test targets, to the other test targets, and to everything else.
 *   It decides whether splitting the compile by test target is worth the manifest churn.
 *
 * Parsers take file *contents*, so they need no cargo and no repository.
 */

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

/** Per-binary run time, slowest first. */
export function perBinaryTimings(junitXml: string): BinaryTiming[] {
  const byBinary = new Map<string, BinaryTiming>();
  // A self-closing `<testsuite .../>` holds no cases; `[^/>]>` keeps it from swallowing the next suite.
  const suite = /<testsuite\s[^>]*?\bname="([^"]*)"[^>]*[^/>]>([\s\S]*?)<\/testsuite>/g;
  for (const [, suiteName, inner] of junitXml.matchAll(suite)) {
    const binary = binaryOfSuite(suiteName);
    const row = byBinary.get(binary) ?? { binary, seconds: 0, tests: 0 };
    for (const [, time] of inner.matchAll(/<testcase\b[^>]*?\btime="([\d.eE+-]+)"/g)) {
      row.seconds += Number(time);
      row.tests += 1;
    }
    byBinary.set(binary, row);
  }
  return [...byBinary.values()]
    .filter((row) => row.tests > 0)
    .sort((a, b) => b.seconds - a.seconds || a.binary.localeCompare(b.binary));
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
  const matches = parseFilterset(filterset);
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
    if (matches({ pkg: unit.name, binary })) e2eTestSeconds += unit.duration;
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

type Subject = { pkg: string; binary: string };

/**
 * Evaluate a nextest filterset over a test target. Supports `or`, `and`, `not`, parentheses and the
 * `binary()`, `package()` and `kind(test)` predicates — everything `.config/rust-e2e.filterset`
 * uses. Anything else is an error: guessing would put a wrong number in the verdict.
 */
function parseFilterset(filterset: string): (subject: Subject) => boolean {
  const tokens = filterset.match(/\(|\)|[A-Za-z_]+\([^()]*\)|[A-Za-z_]+/g) ?? [];
  let at = 0;
  const peek = () => tokens[at];
  const parseOr = (): ((s: Subject) => boolean) => {
    let left = parseAnd();
    while (peek() === "or") {
      at++;
      const [l, r] = [left, parseAnd()];
      left = (s) => l(s) || r(s);
    }
    return left;
  };
  const parseAnd = (): ((s: Subject) => boolean) => {
    let left = parseNot();
    while (peek() === "and") {
      at++;
      const [l, r] = [left, parseNot()];
      left = (s) => l(s) && r(s);
    }
    return left;
  };
  const parseNot = (): ((s: Subject) => boolean) => {
    if (peek() === "not") {
      at++;
      const inner = parseNot();
      return (s) => !inner(s);
    }
    return parseAtom();
  };
  const parseAtom = (): ((s: Subject) => boolean) => {
    const token = tokens[at++];
    if (token === "(") {
      const inner = parseOr();
      if (tokens[at++] !== ")") throw new Error("unbalanced parentheses in the filterset");
      return inner;
    }
    const predicate = /^(\w+)\(([^()]*)\)$/.exec(token ?? "");
    if (!predicate) throw new Error(`unexpected filterset token: ${token ?? "end of input"}`);
    const [, name, arg] = predicate;
    if (name === "binary") return (s) => s.binary === arg;
    if (name === "package") return (s) => s.pkg === arg;
    // Every unit in a `--no-run` report that reaches this check is a test target.
    if (name === "kind" && arg === "test") return () => true;
    throw new Error(`unsupported filterset predicate: ${token}`);
  };
  const matches = parseOr();
  if (at < tokens.length) throw new Error(`unexpected filterset token: ${tokens[at]}`);
  return matches;
}

/** Binaries a filterset names with `binary(...)`, in order of appearance. */
function filtersetBinaries(filterset: string): string[] {
  return [...filterset.matchAll(/\bbinary\(([^()]*)\)/g)].map((m) => m[1]);
}

/** `proceed` when the leg would skip at least {@link SPLIT_WORTH_IT_PERCENT}% of its compile. */
export function verdict(skippablePercent: number): Verdict {
  return skippablePercent >= SPLIT_WORTH_IT_PERCENT ? "proceed" : "stop";
}

function junitSummary(junitXml: string): string {
  const rows = perBinaryTimings(junitXml);
  const total = rows.reduce((sum, row) => sum + row.seconds, 0);
  const lines = ["### e2e run time per test binary", "", "| Binary | Seconds | Tests |", "|---|---:|---:|"];
  for (const row of rows) lines.push(`| ${row.binary} | ${row.seconds.toFixed(1)} | ${row.tests} |`);
  lines.push(`| **total** | **${total.toFixed(1)}** | **${rows.reduce((sum, row) => sum + row.tests, 0)}** |`);
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
  if (command === "junit" && args.length === 1) {
    console.log(junitSummary(await read(args[0])));
  } else if (command === "compile-share" && args.length === 2) {
    console.log(compileShareSummary(compileShare(await read(args[0]), await read(args[1]))));
  } else {
    console.error("usage: ci-e2e-timing.ts junit <junit.xml> | compile-share <cargo-timing.html> <filterset>");
    process.exit(2);
  }
}
