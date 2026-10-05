/**
 * Which test binaries start the LiveKit testkit, and which of them the nextest `docker` group
 * still serialises.
 *
 * The group exists because every test used to launch its own container on ports found by
 * `bind(:0)` and release. With one shared server (`LIVEKIT_TESTKIT_WS_URL`) and a unique room per
 * test, that race is gone and the group has nothing left to protect. Both sets are derived from
 * the source rather than a list a person maintains, so the config cannot drift from the tests again.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

export interface TestBinary {
  /** Cargo package that owns the test target. */
  package: string;
  /** The test target's name: the file name under `tests/`, without `.rs`. */
  binary: string;
}

export interface DockerGroup {
  /** Packages named wholesale: every test target they own is in the group. */
  packages: Set<string>;
  /** `package` + `binary` pairs named individually. */
  binaries: TestBinary[];
  /** Binaries named by `binary(...)` alone: that target in whichever package owns it. */
  unqualifiedBinaries: Set<string>;
}

/** The only way a test brings up (or joins) a LiveKit server: `tddy-livekit-testkit`'s constructor. */
const STARTS_THE_TESTKIT = /\bLiveKitTestkit::start\b/;

/** Every integration-test binary under `packages/<pkg>/tests/` whose source starts the testkit. */
export function binariesStartingTheTestkit(repoRoot: string): TestBinary[] {
  const packagesDir = join(repoRoot, "packages");
  const found: TestBinary[] = [];
  for (const dir of readdirSync(packagesDir).sort()) {
    const manifest = join(packagesDir, dir, "Cargo.toml");
    const testsDir = join(packagesDir, dir, "tests");
    if (!existsSync(manifest) || !isDirectory(testsDir)) continue;
    const pkg = cargoPackageName(readFileSync(manifest, "utf8"), manifest);
    for (const target of testTargets(testsDir)) {
      if (target.sources.some((file) => STARTS_THE_TESTKIT.test(readFileSync(file, "utf8")))) {
        found.push({ package: pkg, binary: target.binary });
      }
    }
  }
  return found;
}

/**
 * Cargo's auto-discovered test targets — `tests/<name>.rs` and `tests/<name>/main.rs` — each with
 * the files compiled into it: its root plus the directory modules it declares with `mod <name>;`.
 */
function testTargets(testsDir: string): { binary: string; sources: string[] }[] {
  const targets: { binary: string; sources: string[] }[] = [];
  for (const entry of readdirSync(testsDir).sort()) {
    const path = join(testsDir, entry);
    if (entry.endsWith(".rs") && !isDirectory(path)) {
      const binary = entry.slice(0, -".rs".length);
      targets.push({ binary, sources: [path, ...declaredModuleFiles(path, testsDir)] });
    } else if (isDirectory(path) && existsSync(join(path, "main.rs"))) {
      targets.push({ binary: entry, sources: rustFilesUnder(path) });
    }
  }
  return targets;
}

/** Files of the `mod <name>;` modules a test root pulls in from `tests/<name>/`. */
function declaredModuleFiles(root: string, testsDir: string): string[] {
  const files: string[] = [];
  for (const [, name] of readFileSync(root, "utf8").matchAll(/^\s*(?:pub\s+)?mod\s+(\w+)\s*;/gm)) {
    const moduleDir = join(testsDir, name);
    if (isDirectory(moduleDir)) files.push(...rustFilesUnder(moduleDir));
  }
  return files;
}

function rustFilesUnder(dir: string): string[] {
  return readdirSync(dir)
    .sort()
    .flatMap((entry) => {
      const path = join(dir, entry);
      if (isDirectory(path)) return rustFilesUnder(path);
      return entry.endsWith(".rs") ? [path] : [];
    });
}

function isDirectory(path: string): boolean {
  return existsSync(path) && statSync(path).isDirectory();
}

/** `name` of the `[package]` table. */
function cargoPackageName(manifest: string, path: string): string {
  let inPackage = false;
  for (const line of manifest.split("\n")) {
    const header = /^\s*\[([^\]]+)\]/.exec(line);
    if (header) {
      inPackage = header[1].trim() === "package";
      continue;
    }
    const name = /^\s*name\s*=\s*"([^"]+)"/.exec(line);
    if (inPackage && name) return name[1];
  }
  throw new Error(`no [package] name in ${path}`);
}

/**
 * What the `docker` override of the `ci` profile in `.config/nextest.toml` selects.
 *
 * Overrides of `default` count too: a profile inherits them, so a `default` override that sets the
 * group serialises the `ci` run as well. No override setting `test-group = "docker"` is an empty
 * group.
 */
export function dockerGroup(nextestToml: string): DockerGroup {
  const group: DockerGroup = { packages: new Set(), binaries: [], unqualifiedBinaries: new Set() };
  for (const override of overrideTables(nextestToml)) {
    if (override["test-group"] !== "docker") continue;
    if (override.filter === undefined) {
      throw new Error("an override sets test-group = \"docker\" without a filter");
    }
    for (const term of disjuncts(parseFilterset(override.filter))) addTerm(group, term);
  }
  return group;
}

/** Whether nextest would serialise `binary` under `group`. */
export function isSerialised(group: DockerGroup, binary: TestBinary): boolean {
  return (
    group.packages.has(binary.package) ||
    group.unqualifiedBinaries.has(binary.binary) ||
    group.binaries.some((named) => named.package === binary.package && named.binary === binary.binary)
  );
}

interface OverrideTable {
  filter?: string;
  "test-group"?: string;
}

/** Every `[[profile.ci.overrides]]` and `[[profile.default.overrides]]` table. */
function overrideTables(toml: string): OverrideTable[] {
  const config = Bun.TOML.parse(toml) as { profile?: Record<string, { overrides?: OverrideTable[] }> };
  return ["ci", "default"].flatMap((profile) => config.profile?.[profile]?.overrides ?? []);
}

type Filter =
  | { kind: "or" | "and"; left: Filter; right: Filter }
  | { kind: "not"; inner: Filter }
  | { kind: "predicate"; name: string; arg: string };

/** A nextest filterset's `or` / `and` / `not` / parentheses / `name(arg)` structure. */
function parseFilterset(filterset: string): Filter {
  const tokens = filterset.match(/\(|\)|[A-Za-z_]+\([^()]*\)|[A-Za-z_]+/g) ?? [];
  let at = 0;
  const binary = (kind: "or" | "and", next: () => Filter) => (): Filter => {
    let left = next();
    while (tokens[at] === kind) {
      at++;
      left = { kind, left, right: next() };
    }
    return left;
  };
  const parseAtom = (): Filter => {
    const token = tokens[at++];
    if (token === "not") return { kind: "not", inner: parseAtom() };
    if (token === "(") {
      const inner = parseOr();
      if (tokens[at++] !== ")") throw new Error("unbalanced parentheses in the filterset");
      return inner;
    }
    const predicate = /^(\w+)\(([^()]*)\)$/.exec(token ?? "");
    if (!predicate) throw new Error(`unexpected filterset token: ${token ?? "end of input"}`);
    return { kind: "predicate", name: predicate[1], arg: predicate[2].trim() };
  };
  const parseAnd = binary("and", parseAtom);
  const parseOr = binary("or", parseAnd);
  const filter = parseOr();
  if (at < tokens.length) throw new Error(`unexpected filterset token: ${tokens[at]}`);
  return filter;
}

function disjuncts(filter: Filter): Filter[] {
  return filter.kind === "or" ? [...disjuncts(filter.left), ...disjuncts(filter.right)] : [filter];
}

/**
 * One `or` term of the group's filter: `package(p)`, `binary(b)` or
 * `package(p) and (binary(a) or binary(b) ...)`. Any other shape is an error — reading it as
 * something narrower would report a binary as parallel that nextest still serialises.
 */
function addTerm(group: DockerGroup, term: Filter): void {
  const pkg = predicateArg(term, "package");
  if (pkg !== undefined) {
    group.packages.add(pkg);
    return;
  }
  const bin = predicateArg(term, "binary");
  if (bin !== undefined) {
    group.unqualifiedBinaries.add(bin);
    return;
  }
  if (term.kind === "and") {
    const owner = predicateArg(term.left, "package");
    const names = disjuncts(term.right).map((filter) => predicateArg(filter, "binary"));
    if (owner !== undefined && names.every((name) => name !== undefined)) {
      for (const name of names) group.binaries.push({ package: owner, binary: name as string });
      return;
    }
  }
  throw new Error(`unsupported term in the docker group's filter: ${JSON.stringify(term)}`);
}

/** The exact name a `name(arg)` predicate matches, or `undefined` if `filter` is not one. */
function predicateArg(filter: Filter, name: string): string | undefined {
  if (filter.kind !== "predicate" || filter.name !== name) return undefined;
  const arg = filter.arg.startsWith("=") ? filter.arg.slice(1) : filter.arg;
  if (/[*?[\]~/]/.test(arg)) {
    throw new Error(`only exact names are supported in the docker group's filter: ${filter.name}(${filter.arg})`);
  }
  return arg;
}
