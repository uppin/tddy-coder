/**
 * Index of packages already present in this machine's bun, npm and yarn caches,
 * restricted to tarballs that were fetched from one registry host.
 *
 * `resolve-local-lock.ts` asks the registry whether each locked `name@version`
 * exists there. A cached tarball from that same host answers the question
 * without a request: the registry served that exact version. Caches filled
 * from another registry prove nothing about this one and are ignored.
 *
 * Zero external dependencies — reads the caches' on-disk formats directly.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "fs";
import { homedir, platform } from "os";
import { join } from "path";

export type CacheSource = "bun" | "npm" | "yarn";

export interface CachedPackage {
  source: CacheSource;
  /** The tarball's integrity, when the cache records one (bun does not). */
  integrity?: string;
}

export interface CacheDirs {
  bun?: string;
  npm?: string;
  yarn?: string;
}

/** The cache directories each package manager uses by default, honouring their env overrides. */
export function defaultCacheDirs(): CacheDirs {
  const home = homedir();
  return {
    bun:
      process.env.BUN_INSTALL_CACHE_DIR ??
      join(process.env.BUN_INSTALL ?? join(home, ".bun"), "install", "cache"),
    npm: join(process.env.npm_config_cache ?? join(home, ".npm"), "_cacache"),
    yarn:
      process.env.YARN_CACHE_FOLDER ??
      (platform() === "darwin"
        ? join(home, "Library", "Caches", "Yarn", "v6")
        : join(process.env.XDG_CACHE_HOME ?? join(home, ".cache"), "yarn", "v6")),
  };
}

/**
 * Builds a `name@version` → cached package map for tarballs fetched from
 * `registryUrl`'s host. The first cache to know a package wins, in the order
 * npm, yarn, bun — so an entry carries an integrity whenever any cache has one.
 */
export function loadLocalPackageCache(
  registryUrl: string,
  dirs: CacheDirs = defaultCacheDirs()
): Map<string, CachedPackage> {
  const host = new URL(registryUrl).host;
  const index = new Map<string, CachedPackage>();
  const add = (name: string, version: string, entry: CachedPackage) => {
    const key = `${name}@${version}`;
    if (!index.has(key)) index.set(key, entry);
  };

  if (dirs.npm) readNpmCache(dirs.npm, host, add);
  if (dirs.yarn) readYarnCache(dirs.yarn, host, add);
  if (dirs.bun) readBunCache(dirs.bun, host, add);
  return index;
}

type Add = (name: string, version: string, entry: CachedPackage) => void;

// ---------------------------------------------------------------------------
// npm: cacache index lines `<sha1>\t<json>` keyed by the tarball URL
// ---------------------------------------------------------------------------

const NPM_KEY_PREFIX = "make-fetch-happen:request-cache:";

function readNpmCache(cacheDir: string, host: string, add: Add) {
  const indexDir = join(cacheDir, "index-v5");
  for (const file of walkFiles(indexDir)) {
    for (const line of readFileSync(file, "utf-8").split("\n")) {
      const tab = line.indexOf("\t");
      if (tab < 0) continue;
      let record: { key?: string; integrity?: string };
      try {
        record = JSON.parse(line.slice(tab + 1));
      } catch {
        continue;
      }
      if (!record.key?.startsWith(NPM_KEY_PREFIX) || !record.integrity) continue;
      const pkg = parseTarballUrl(record.key.slice(NPM_KEY_PREFIX.length), host);
      if (pkg) add(pkg.name, pkg.version, { source: "npm", integrity: record.integrity });
    }
  }
}

/**
 * `https://<host>/<any prefix>/<name>/-/<file>-<version>.tgz` → name and version,
 * where `<name>` may be scoped (`@scope/pkg`, possibly URL-encoded) and `<file>`
 * is the name's basename or the full scoped name.
 */
export function parseTarballUrl(
  url: string,
  host: string
): { name: string; version: string } | null {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.host !== host || !parsed.pathname.endsWith(".tgz")) return null;

  const path = decodeURIComponent(parsed.pathname);
  const sep = path.lastIndexOf("/-/");
  if (sep < 0) return null;

  const segments = path.slice(0, sep).split("/").filter(Boolean);
  const last = segments.at(-1);
  if (!last) return null;
  const scope = segments.at(-2);
  const name = last.includes("/")
    ? last
    : scope?.startsWith("@")
      ? `${scope}/${last}`
      : last;

  const file = path.slice(sep + 3, -".tgz".length);
  const basename = name.split("/").at(-1)!;
  for (const prefix of [`${name}-`, `${basename}-`]) {
    if (file.startsWith(prefix)) {
      return { name, version: file.slice(prefix.length) };
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// yarn (v1): `<cache>/npm-…/node_modules/<name>/.yarn-metadata.json`
// ---------------------------------------------------------------------------

function readYarnCache(cacheDir: string, host: string, add: Add) {
  for (const entry of listDir(cacheDir)) {
    if (!entry.startsWith("npm-")) continue;
    const modules = join(cacheDir, entry, "node_modules");
    for (const pkgDir of packageDirs(modules)) {
      const metadataPath = join(pkgDir, ".yarn-metadata.json");
      if (!existsSync(metadataPath)) continue;
      let metadata: {
        manifest?: { name?: string; version?: string };
        remote?: { resolved?: string; integrity?: string };
      };
      try {
        metadata = JSON.parse(readFileSync(metadataPath, "utf-8"));
      } catch {
        continue;
      }
      const { name, version } = metadata.manifest ?? {};
      const resolved = metadata.remote?.resolved;
      if (!name || !version || !resolved || hostOf(resolved) !== host) continue;
      add(name, version, { source: "yarn", integrity: metadata.remote?.integrity });
    }
  }
}

// ---------------------------------------------------------------------------
// bun: `<cache>/[@scope/]<name>@<version>@@<host>@@@<n>` (no host for npmjs)
// ---------------------------------------------------------------------------

const BUN_DEFAULT_REGISTRY_HOST = "registry.npmjs.org";

function readBunCache(cacheDir: string, host: string, add: Add) {
  for (const entry of listDir(cacheDir)) {
    if (entry.startsWith("@")) {
      for (const scoped of listDir(join(cacheDir, entry))) {
        const pkg = parseBunCacheEntry(`${entry}/${scoped}`, host);
        if (pkg) add(pkg.name, pkg.version, { source: "bun" });
      }
    } else {
      const pkg = parseBunCacheEntry(entry, host);
      if (pkg) add(pkg.name, pkg.version, { source: "bun" });
    }
  }
}

export function parseBunCacheEntry(
  entry: string,
  host: string
): { name: string; version: string } | null {
  const m = entry.match(/^(.+)@@@\d+$/);
  if (!m) return null;
  let spec = m[1];
  let entryHost = BUN_DEFAULT_REGISTRY_HOST;
  const hostSep = spec.lastIndexOf("@@");
  if (hostSep >= 0) {
    entryHost = spec.slice(hostSep + 2);
    spec = spec.slice(0, hostSep);
  }
  if (entryHost !== host) return null;
  const at = spec.lastIndexOf("@");
  if (at <= 0) return null;
  return { name: spec.slice(0, at), version: spec.slice(at + 1) };
}

// ---------------------------------------------------------------------------
// fs helpers
// ---------------------------------------------------------------------------

function listDir(dir: string): string[] {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
}

function* walkFiles(dir: string): Generator<string> {
  for (const entry of listDir(dir)) {
    const path = join(dir, entry);
    let stats;
    try {
      stats = statSync(path);
    } catch {
      continue;
    }
    if (stats.isDirectory()) yield* walkFiles(path);
    else if (stats.isFile()) yield path;
  }
}

function* packageDirs(modulesDir: string): Generator<string> {
  for (const entry of listDir(modulesDir)) {
    if (entry.startsWith("@")) {
      for (const scoped of listDir(join(modulesDir, entry))) {
        yield join(modulesDir, entry, scoped);
      }
    } else {
      yield join(modulesDir, entry);
    }
  }
}

function hostOf(url: string): string | null {
  try {
    return new URL(url).host;
  } catch {
    return null;
  }
}
