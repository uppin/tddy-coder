/**
 * Resolves bun.lock package versions against a local npm registry.
 *
 * Usage:
 *   LOCAL_REGISTRY_URL=https://npm.dev.wixpress.com bun run scripts/resolve-local-lock.ts
 *
 * Reads `bun.lock` from the project root, queries the local registry for
 * each package, finds the best available version (exact match preferred,
 * then highest version <= locked version, then latest available), and
 * writes the result to `local.bun.lock`.
 *
 * Zero external dependencies — semver logic is inlined.
 */

import { readFileSync, writeFileSync } from "fs";
import { join } from "path";

const REGISTRY_URL =
  process.env.LOCAL_REGISTRY_URL ?? "https://npm.dev.wixpress.com";
const LOCK_PATH = join(import.meta.dir, "..", "bun.lock");
const OUTPUT_PATH = join(import.meta.dir, "..", "local.bun.lock");
const CONCURRENCY = 10;

// ---------------------------------------------------------------------------
// Minimal semver helpers (no deps)
// ---------------------------------------------------------------------------

interface SemVer {
  major: number;
  minor: number;
  patch: number;
  prerelease: string[];
  raw: string;
}

function parseSemVer(v: string): SemVer | null {
  const m = v.match(
    /^v?(\d+)\.(\d+)\.(\d+)(?:-([a-zA-Z0-9.]+))?/
  );
  if (!m) return null;
  return {
    major: Number(m[1]),
    minor: Number(m[2]),
    patch: Number(m[3]),
    prerelease: m[4] ? m[4].split(".") : [],
    raw: v,
  };
}

function compareSemVer(a: SemVer, b: SemVer): number {
  if (a.major !== b.major) return a.major - b.major;
  if (a.minor !== b.minor) return a.minor - b.minor;
  if (a.patch !== b.patch) return a.patch - b.patch;
  if (a.prerelease.length === 0 && b.prerelease.length === 0) return 0;
  if (a.prerelease.length === 0) return 1;
  if (b.prerelease.length === 0) return -1;
  for (let i = 0; i < Math.max(a.prerelease.length, b.prerelease.length); i++) {
    if (i >= a.prerelease.length) return -1;
    if (i >= b.prerelease.length) return 1;
    const ai = a.prerelease[i];
    const bi = b.prerelease[i];
    const an = Number(ai);
    const bn = Number(bi);
    if (!isNaN(an) && !isNaN(bn)) {
      if (an !== bn) return an - bn;
    } else {
      const cmp = ai.localeCompare(bi);
      if (cmp !== 0) return cmp;
    }
  }
  return 0;
}

function semverLte(a: SemVer, b: SemVer): boolean {
  return compareSemVer(a, b) <= 0;
}

// ---------------------------------------------------------------------------
// Bun lockfile types
// ---------------------------------------------------------------------------

type LockfilePackageEntry = [
  string, // e.g. "@babel/core@7.29.0"
  string, // tarball path or ""
  Record<string, unknown>, // metadata
  string, // integrity hash
];

interface Lockfile {
  lockfileVersion: number;
  configVersion: number;
  workspaces: Record<string, unknown>;
  packages: Record<string, LockfilePackageEntry>;
}

// ---------------------------------------------------------------------------
// JSONC parser (strips trailing commas for JSON.parse)
// ---------------------------------------------------------------------------

function stripJsoncTrailingCommas(text: string): string {
  return text.replace(/,(\s*[}\]])/g, "$1");
}

function parseLockfile(path: string): Lockfile {
  const raw = readFileSync(path, "utf-8");
  return JSON.parse(stripJsoncTrailingCommas(raw)) as Lockfile;
}

function parsePackageId(id: string): { name: string; version: string } {
  const atIndex = id.lastIndexOf("@");
  if (atIndex <= 0) {
    throw new Error(`Cannot parse package id: ${id}`);
  }
  return {
    name: id.slice(0, atIndex),
    version: id.slice(atIndex + 1),
  };
}

// ---------------------------------------------------------------------------
// Registry client
// ---------------------------------------------------------------------------

interface RegistryVersionInfo {
  version: string;
  dist?: {
    integrity?: string;
    shasum?: string;
    tarball?: string;
  };
}

interface RegistryResponse {
  name: string;
  versions: Record<string, RegistryVersionInfo>;
  "dist-tags"?: Record<string, string>;
}

const registryCache = new Map<string, RegistryResponse | null>();

async function fetchRegistryInfo(
  packageName: string
): Promise<RegistryResponse | null> {
  if (registryCache.has(packageName)) {
    return registryCache.get(packageName)!;
  }

  const url = `${REGISTRY_URL}/${packageName}`;
  const accept = packageName.startsWith("@")
    ? "application/json"
    : "application/vnd.npm.install-v1+json";

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 30_000);
    const response = await fetch(url, {
      headers: { Accept: accept },
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (!response.ok) {
      console.warn(`  [WARN] ${packageName}: registry returned ${response.status}`);
      registryCache.set(packageName, null);
      return null;
    }

    const data = (await response.json()) as RegistryResponse;
    registryCache.set(packageName, data);
    return data;
  } catch (err) {
    console.warn(
      `  [WARN] ${packageName}: fetch failed -`,
      (err as Error).message
    );
    registryCache.set(packageName, null);
    return null;
  }
}

// ---------------------------------------------------------------------------
// Version resolution
// ---------------------------------------------------------------------------

function findBestVersion(
  registryInfo: RegistryResponse,
  wantedVersion: string
): string | null {
  const available = Object.keys(registryInfo.versions);
  if (available.length === 0) return null;

  if (available.includes(wantedVersion)) {
    return wantedVersion;
  }

  const wantedParsed = parseSemVer(wantedVersion);
  if (!wantedParsed) {
    const latest = registryInfo["dist-tags"]?.latest;
    return latest && available.includes(latest) ? latest : available[available.length - 1];
  }

  const parsed = available
    .map((v) => parseSemVer(v))
    .filter((v): v is SemVer => v !== null);

  parsed.sort(compareSemVer);

  let best: SemVer | null = null;
  for (const sv of parsed) {
    if (semverLte(sv, wantedParsed)) {
      best = sv;
    }
  }

  if (best) return best.raw;

  return parsed.length > 0 ? parsed[parsed.length - 1].raw : null;
}

// ---------------------------------------------------------------------------
// Batch processing
// ---------------------------------------------------------------------------

async function processInBatches<T, R>(
  items: T[],
  concurrency: number,
  fn: (item: T) => Promise<R>
): Promise<R[]> {
  const results: R[] = [];
  for (let i = 0; i < items.length; i += concurrency) {
    const batch = items.slice(i, i + concurrency);
    const batchResults = await Promise.all(batch.map(fn));
    results.push(...batchResults);
  }
  return results;
}

// ---------------------------------------------------------------------------
// Resolve a single package
// ---------------------------------------------------------------------------

interface ResolvedPackage {
  key: string;
  entry: LockfilePackageEntry;
  changed: boolean;
}

async function resolvePackage(
  key: string,
  entry: LockfilePackageEntry
): Promise<ResolvedPackage> {
  const packageId = entry[0];

  if (
    packageId.startsWith("workspace:") ||
    packageId.includes("link:") ||
    packageId.includes("file:")
  ) {
    return { key, entry, changed: false };
  }

  const { name, version: lockedVersion } = parsePackageId(packageId);

  const registryInfo = await fetchRegistryInfo(name);
  if (!registryInfo) {
    return { key, entry, changed: false };
  }

  const bestVersion = findBestVersion(registryInfo, lockedVersion);
  if (!bestVersion || bestVersion === lockedVersion) {
    return { key, entry, changed: false };
  }

  const versionInfo = registryInfo.versions[bestVersion];
  const newPackageId = `${name}@${bestVersion}`;
  const newIntegrity = versionInfo?.dist?.integrity ?? entry[3];

  const newEntry: LockfilePackageEntry = [
    newPackageId,
    entry[1],
    entry[2],
    newIntegrity,
  ];

  return { key, entry: newEntry, changed: true };
}

// ---------------------------------------------------------------------------
// Workspace dependency version resolution
// ---------------------------------------------------------------------------

function resolveWorkspaceDeps(
  lockfile: Lockfile,
  resolvedPackages: Record<string, LockfilePackageEntry>
): Record<string, unknown> {
  const newWorkspaces: Record<string, unknown> = {};

  for (const [wsKey, wsValue] of Object.entries(lockfile.workspaces)) {
    const ws = wsValue as Record<string, unknown>;
    const newWs = { ...ws };

    for (const depField of ["dependencies", "devDependencies", "optionalDependencies"] as const) {
      const deps = ws[depField] as Record<string, string> | undefined;
      if (!deps) continue;

      const newDeps: Record<string, string> = {};
      for (const [depName, depRange] of Object.entries(deps)) {
        if (depRange.startsWith("workspace:")) {
          newDeps[depName] = depRange;
          continue;
        }

        const pkgEntry = resolvedPackages[depName];
        if (pkgEntry) {
          const { version } = parsePackageId(pkgEntry[0]);
          newDeps[depName] = version;
        } else {
          newDeps[depName] = depRange;
        }
      }
      newWs[depField] = newDeps;
    }

    newWorkspaces[wsKey] = newWs;
  }

  return newWorkspaces;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log(`Reading lockfile: ${LOCK_PATH}`);
  console.log(`Registry: ${REGISTRY_URL}`);
  console.log();

  const lockfile = parseLockfile(LOCK_PATH);
  const packageEntries = Object.entries(lockfile.packages);

  console.log(`Found ${packageEntries.length} packages to resolve`);
  console.log();

  let changedCount = 0;
  let unchangedCount = 0;
  let errorCount = 0;
  let processedCount = 0;
  const total = packageEntries.length;

  const results = await processInBatches(
    packageEntries,
    CONCURRENCY,
    async ([key, entry]) => {
      try {
        const result = await resolvePackage(key, entry as LockfilePackageEntry);
        processedCount++;
        if (processedCount % 50 === 0 || processedCount === total) {
          process.stdout.write(`\r  Progress: ${processedCount}/${total}`);
        }
        return result;
      } catch (err) {
        console.warn(`  [ERROR] ${key}: ${(err as Error).message}`);
        errorCount++;
        processedCount++;
        return {
          key,
          entry: entry as LockfilePackageEntry,
          changed: false,
        };
      }
    }
  );
  console.log(); // newline after progress

  const newPackages: Record<string, LockfilePackageEntry> = {};

  for (const result of results) {
    newPackages[result.key] = result.entry;
    if (result.changed) {
      const origEntry = lockfile.packages[result.key] as LockfilePackageEntry;
      const { name: origName, version: origVersion } = parsePackageId(origEntry[0]);
      const { version: newVersion } = parsePackageId(result.entry[0]);
      console.log(`  [CHANGED] ${origName}: ${origVersion} -> ${newVersion}`);
      changedCount++;
    } else {
      unchangedCount++;
    }
  }

  const newWorkspaces = resolveWorkspaceDeps(lockfile, newPackages);

  const newLockfile: Lockfile = {
    lockfileVersion: lockfile.lockfileVersion,
    configVersion: lockfile.configVersion,
    workspaces: newWorkspaces,
    packages: newPackages,
  };

  const output = JSON.stringify(newLockfile, null, 2) + "\n";
  writeFileSync(OUTPUT_PATH, output);

  console.log();
  console.log("Summary:");
  console.log(`  Changed:   ${changedCount}`);
  console.log(`  Unchanged: ${unchangedCount}`);
  console.log(`  Errors:    ${errorCount}`);
  console.log(`  Output:    ${OUTPUT_PATH}`);
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
