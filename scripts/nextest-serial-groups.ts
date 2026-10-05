/**
 * Which test binaries start the LiveKit testkit, and which of them a serial nextest test-group
 * (`max-threads = 1`) still holds in the `ci` profile.
 *
 * LiveKit tests used to be serialised because each launched its own container on ports found by
 * `bind(:0)` and release. With one shared server (`LIVEKIT_TESTKIT_WS_URL`) and a unique room per
 * test, that race is gone and no LiveKit test belongs in a serial group. Both sides are derived —
 * the testkit users from the test sources, the serialised set from `.config/nextest.toml` — rather
 * than from a list a person maintains, so the config cannot drift from the tests again.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import {
  type Filter,
  type FilterReference,
  type FilterSubject,
  filterReferences,
  matches,
  parseFilterset,
} from "./nextest-filterset";

export type TestBinary = FilterSubject;

/** An auto-discovered integration-test target and the files compiled into it. */
export interface TestTarget extends TestBinary {
  sources: string[];
}

/** What the `ci` profile does with test-groups: which groups are serial, and who is put in one. */
export interface CiSerialisation {
  /** Test-groups declared with `max-threads = 1`. */
  serialGroups: Set<string>;
  /** Overrides that set `test-group`, in the order nextest applies them: `ci`'s, then `default`'s. */
  overrides: { filter: Filter; testGroup: string }[];
}

/** The only way a test brings up (or joins) a LiveKit server: `tddy-livekit-testkit`'s constructor. */
const STARTS_THE_TESTKIT = /\bLiveKitTestkit::start\b/;

/** Every integration-test binary of the workspace whose source starts the testkit. */
export function binariesStartingTheTestkit(repoRoot: string): TestBinary[] {
  return testBinaries(repoRoot)
    .filter((target) => target.sources.some((file) => STARTS_THE_TESTKIT.test(readFileSync(file, "utf8"))))
    .map(({ package: pkg, binary }) => ({ package: pkg, binary }));
}

/** Every auto-discovered integration-test target of every workspace member, by package. */
export function testBinaries(repoRoot: string): TestTarget[] {
  return workspacePackages(repoRoot).flatMap(({ name, dir }) => packageTestTargets(name, dir));
}

/** Name and directory of every member of the workspace rooted at `repoRoot`. */
export function workspacePackages(repoRoot: string): { name: string; dir: string }[] {
  const manifest = readManifest(join(repoRoot, "Cargo.toml"));
  const members = manifest.workspace?.members;
  if (!Array.isArray(members)) throw new Error(`no [workspace] members in ${join(repoRoot, "Cargo.toml")}`);
  const excluded = new Set((manifest.workspace?.exclude ?? []).map((path) => join(repoRoot, path)));
  const dirs = members.flatMap((pattern) => expandMember(repoRoot, pattern)).filter((dir) => !excluded.has(dir));
  return [...new Set(dirs)].sort().map((dir) => ({ name: packageName(join(dir, "Cargo.toml")), dir }));
}

interface Manifest {
  workspace?: { members?: string[]; exclude?: string[] };
  package?: { name?: unknown; autotests?: unknown };
  test?: unknown;
}

function readManifest(path: string): Manifest {
  return Bun.TOML.parse(readFileSync(path, "utf8")) as Manifest;
}

/**
 * The directories a `members` entry names. A glob (`*`, `?`) matches directory names one path
 * segment at a time, as cargo's does.
 */
function expandMember(repoRoot: string, pattern: string): string[] {
  let dirs = [repoRoot];
  for (const segment of pattern.split("/").filter((part) => part !== "" && part !== ".")) {
    if (!/[*?[\]]/.test(segment)) {
      dirs = dirs.map((dir) => join(dir, segment));
      continue;
    }
    if (/[[\]]/.test(segment)) throw new Error(`unsupported glob in workspace member "${pattern}"`);
    const matcher = new RegExp(`^${segment.replace(/[.+^${}()|\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".")}$`);
    dirs = dirs.flatMap((dir) =>
      readdirSync(dir)
        .filter((entry) => matcher.test(entry) && isDirectory(join(dir, entry)))
        .map((entry) => join(dir, entry)),
    );
  }
  for (const dir of dirs) {
    if (!existsSync(join(dir, "Cargo.toml"))) throw new Error(`workspace member ${dir} has no Cargo.toml`);
  }
  return dirs;
}

/** `[package] name`. */
function packageName(manifestPath: string): string {
  const name = readManifest(manifestPath).package?.name;
  if (typeof name !== "string") throw new Error(`no [package] name in ${manifestPath}`);
  return name;
}

/**
 * Cargo's auto-discovered test targets of one package — `tests/<name>.rs` and `tests/<name>/main.rs`
 * — each with the files compiled into it. A manifest that declares its own `[[test]]` targets or
 * sets `autotests` is rejected: those targets are named by the manifest, not by the file tree.
 */
export function packageTestTargets(pkg: string, packageDir: string): TestTarget[] {
  const manifestPath = join(packageDir, "Cargo.toml");
  const manifest = readManifest(manifestPath);
  if (manifest.test !== undefined) throw new Error(`${manifestPath} declares [[test]] targets, which are not supported`);
  if (manifest.package?.autotests !== undefined) {
    throw new Error(`${manifestPath} sets autotests, which is not supported`);
  }
  const testsDir = join(packageDir, "tests");
  if (!isDirectory(testsDir)) return [];
  const targets: TestTarget[] = [];
  for (const entry of readdirSync(testsDir).sort()) {
    const path = join(testsDir, entry);
    if (entry.endsWith(".rs") && !isDirectory(path)) {
      targets.push({ package: pkg, binary: entry.slice(0, -".rs".length), sources: moduleTree(path, testsDir) });
    } else if (isDirectory(path) && existsSync(join(path, "main.rs"))) {
      targets.push({ package: pkg, binary: entry, sources: moduleTree(join(path, "main.rs"), path) });
    }
  }
  return targets;
}

/** `file` and every file it pulls in with `mod <name>;`, transitively. */
function moduleTree(file: string, childDir: string): string[] {
  return [file, ...declaredModules(file, childDir).flatMap(({ path, dir }) => moduleTree(path, dir))];
}

/** A `mod <name>;` declaration, with any `#[path = "..."]` attribute (and other attributes) before it. */
const MOD_DECLARATION = /^[ \t]*((?:#\[[^\]]*\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/gm;

/**
 * The files of the out-of-line modules `file` declares. A module `x` declared in a file whose
 * child modules live in `childDir` is `childDir/x.rs` or `childDir/x/mod.rs`; its own children then
 * live in `childDir/x/`. A `#[path]` attribute names the file relative to `file`'s directory.
 */
function declaredModules(file: string, childDir: string): { path: string; dir: string }[] {
  const modules: { path: string; dir: string }[] = [];
  for (const [, attributes, name] of withoutCommentsAndLiterals(readFileSync(file, "utf8")).matchAll(MOD_DECLARATION)) {
    const pathAttribute = /#\[\s*path\s*=\s*"([^"]+)"\s*\]/.exec(attributes)?.[1];
    if (pathAttribute !== undefined) {
      const path = join(dirname(file), pathAttribute);
      modules.push({ path, dir: basename(path) === "mod.rs" ? dirname(path) : path.slice(0, -".rs".length) });
      continue;
    }
    const candidates = [join(childDir, `${name}.rs`), join(childDir, name, "mod.rs")].filter(
      (candidate) => existsSync(candidate) && !isDirectory(candidate),
    );
    if (candidates.length !== 1) {
      const which = candidates.length === 0 ? "neither" : "both";
      throw new Error(`\`mod ${name};\` in ${file}: found ${which} of ${name}.rs and ${name}/mod.rs in ${childDir}`);
    }
    modules.push({ path: candidates[0], dir: join(childDir, name) });
  }
  return modules;
}

/**
 * `source` with its comments and string literals blanked out (newlines kept), so a `mod x;` inside a
 * fixture string or a commented-out line is not read as a declaration. A `#[path = "..."]` string is
 * kept: it is the one literal a declaration needs.
 */
function withoutCommentsAndLiterals(source: string): string {
  let out = "";
  let at = 0;
  while (at < source.length) {
    const end = endOfCommentOrLiteral(source, at);
    if (end === undefined) {
      out += source[at++];
      continue;
    }
    out += source.slice(at, end).replace(/[^\n]/g, " ");
    at = end;
  }
  return out;
}

/** Where the comment or literal starting at `at` ends, or `undefined` if none starts there. */
function endOfCommentOrLiteral(source: string, at: number): number | undefined {
  const after = (index: number) => (index < 0 ? source.length : index);
  if (source.startsWith("//", at)) return after(source.indexOf("\n", at));
  if (source.startsWith("/*", at)) {
    let depth = 0;
    for (let i = at; i < source.length - 1; i++) {
      if (source.startsWith("/*", i)) {
        depth++;
        i++;
      } else if (source.startsWith("*/", i)) {
        if (--depth === 0) return i + 2;
        i++;
      }
    }
    return source.length;
  }
  const raw = /^b?r(#*)"/.exec(source.slice(at, at + 260));
  if (raw && !/\w/.test(source[at - 1] ?? "")) {
    const closing = `"${raw[1]}`;
    const close = source.indexOf(closing, at + raw[0].length);
    return close < 0 ? source.length : close + closing.length;
  }
  if (source[at] === '"' && !/#\[\s*path\s*=\s*$/.test(source.slice(Math.max(0, at - 40), at))) {
    let i = at + 1;
    while (i < source.length && source[i] !== '"') i += source[i] === "\\" ? 2 : 1;
    return i + 1;
  }
  const char = /^'(?:\\.|[^\\\n])'/.exec(source.slice(at, at + 4));
  return char ? at + char[0].length : undefined;
}

function isDirectory(path: string): boolean {
  return existsSync(path) && statSync(path).isDirectory();
}

interface OverrideTable {
  filter?: string;
  platform?: unknown;
  "test-group"?: string;
}

interface NextestConfig {
  "test-groups"?: Record<string, { "max-threads"?: unknown }>;
  profile?: Record<string, { inherits?: unknown; overrides?: OverrideTable[] }>;
}

/** Test-groups declared with `max-threads = 1` in `[test-groups]`. */
export function serialGroups(nextestToml: string): Set<string> {
  const groups = parseNextestConfig(nextestToml)["test-groups"] ?? {};
  return new Set(Object.entries(groups).filter(([, group]) => group["max-threads"] === 1).map(([name]) => name));
}

/**
 * How the `ci` profile of `.config/nextest.toml` assigns test-groups.
 *
 * Overrides of `default` count too: a profile inherits them, after its own, so a `default` override
 * that sets a serial group serialises the `ci` run as well. A `ci` profile that `inherits` from
 * another profile, and an override restricted by `platform`, are rejected rather than guessed at.
 */
export function ciSerialisation(nextestToml: string): CiSerialisation {
  const config = parseNextestConfig(nextestToml);
  if (config.profile?.ci?.inherits !== undefined) {
    throw new Error("the ci profile sets `inherits`, which is not supported: its overrides could come from any profile");
  }
  const overrides = ["ci", "default"]
    .flatMap((profile) => config.profile?.[profile]?.overrides ?? [])
    .filter((override) => override["test-group"] !== undefined)
    .map((override) => {
      if (override.platform !== undefined) {
        throw new Error(`an override sets test-group = "${override["test-group"]}" for a platform, which is not supported`);
      }
      if (override.filter === undefined) {
        throw new Error(`an override sets test-group = "${override["test-group"]}" without a filter`);
      }
      return { filter: parseFilterset(override.filter), testGroup: String(override["test-group"]) };
    });
  return { serialGroups: serialGroups(nextestToml), overrides };
}

function parseNextestConfig(nextestToml: string): NextestConfig {
  return Bun.TOML.parse(nextestToml) as NextestConfig;
}

/**
 * Whether nextest would run `binary` in a serial group under the `ci` profile: the first override
 * that selects it and sets `test-group` decides its group.
 */
export function isSerialised(config: CiSerialisation, binary: TestBinary): boolean {
  const decisive = config.overrides.find((override) => matches(override.filter, binary));
  return decisive !== undefined && config.serialGroups.has(decisive.testGroup);
}

/** Every `package()` and `binary()` name the filters of serial-group overrides mention. */
export function serialGroupReferences(config: CiSerialisation): FilterReference[] {
  return config.overrides
    .filter((override) => config.serialGroups.has(override.testGroup))
    .flatMap((override) => filterReferences(override.filter));
}

/**
 * The references that name nothing in the workspace: a package that is not a member, a qualified
 * binary its package does not have, or an unqualified binary no package has.
 */
export function referencesMatchingNothing(
  references: FilterReference[],
  workspace: { packages: string[]; binaries: TestBinary[] },
): string[] {
  const packages = new Set(workspace.packages);
  const qualified = new Set(workspace.binaries.map(label));
  const unqualified = new Set(workspace.binaries.map((binary) => binary.binary));
  return references
    .filter((ref) => {
      if (ref.kind === "package") return !packages.has(ref.package);
      if (ref.package !== undefined) return !qualified.has(label({ package: ref.package, binary: ref.binary }));
      return !unqualified.has(ref.binary);
    })
    .map((ref) =>
      ref.kind === "package" ? `package(${ref.package})` : ref.package === undefined ? `binary(${ref.binary})` : `${ref.package}::${ref.binary}`,
    );
}

/** `package::binary`, the way the drift check reports a binary. */
export function label(binary: TestBinary): string {
  return `${binary.package}::${binary.binary}`;
}
