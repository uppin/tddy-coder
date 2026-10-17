import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  loadLocalPackageCache,
  parseBunCacheEntry,
  parseTarballUrl,
} from "./local-package-cache";

const REGISTRY = "https://npm.example.com";
const HOST = "npm.example.com";

/** Bun, npm and yarn cache directories laid out the way each package manager writes them. */
class LocalCaches {
  readonly root = mkdtempSync(join(tmpdir(), "local-package-cache-"));
  readonly dirs = {
    bun: join(this.root, "bun"),
    npm: join(this.root, "npm"),
    yarn: join(this.root, "yarn"),
  };
  private npmFiles = 0;

  withBunPackage(dirName: string): this {
    mkdirSync(join(this.dirs.bun, dirName), { recursive: true });
    return this;
  }

  withNpmTarball(url: string, integrity: string): this {
    const dir = join(this.dirs.npm, "index-v5", "ab", "cd");
    mkdirSync(dir, { recursive: true });
    const record = { key: `make-fetch-happen:request-cache:${url}`, integrity };
    writeFileSync(join(dir, `entry${this.npmFiles++}`), `0000\t${JSON.stringify(record)}\n`);
    return this;
  }

  withYarnPackage(name: string, version: string, resolved: string, integrity: string): this {
    const dir = join(this.dirs.yarn, `npm-${name.replace("/", "-")}-${version}-abc-integrity`, "node_modules", name);
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, ".yarn-metadata.json"),
      JSON.stringify({ manifest: { name, version }, remote: { resolved, integrity } }),
    );
    return this;
  }

  index() {
    return loadLocalPackageCache(REGISTRY, this.dirs);
  }

  cleanup() {
    rmSync(this.root, { recursive: true, force: true });
  }
}

let caches: LocalCaches;
afterEach(() => caches?.cleanup());

describe("bun cache entries", () => {
  test("a package fetched from the registry host is named by its directory", () => {
    expect(parseBunCacheEntry("react@19.2.4@@npm.example.com@@@1", HOST)).toEqual({
      name: "react",
      version: "19.2.4",
    });
  });

  test("a scoped package keeps its scope", () => {
    expect(parseBunCacheEntry("@babel/core@7.29.0@@npm.example.com@@@1", HOST)).toEqual({
      name: "@babel/core",
      version: "7.29.0",
    });
  });

  test("a package from another registry is not this registry's", () => {
    expect(parseBunCacheEntry("react@19.2.4@@other.example.com@@@1", HOST)).toBeNull();
  });

  test("a host-less entry belongs to the public npm registry", () => {
    expect(parseBunCacheEntry("react@19.2.4@@@1", "registry.npmjs.org")).toEqual({
      name: "react",
      version: "19.2.4",
    });
    expect(parseBunCacheEntry("react@19.2.4@@@1", HOST)).toBeNull();
  });

  test("a manifest file or bare package directory is not a cached tarball", () => {
    expect(parseBunCacheEntry("0108df8059925cbb-65af37f3f3e5a306.npm", HOST)).toBeNull();
    expect(parseBunCacheEntry("react", HOST)).toBeNull();
  });
});

describe("npm tarball urls", () => {
  test("a tarball under a path prefix yields its name and version", () => {
    expect(
      parseTarballUrl(`${REGISTRY}/api/npm/npm-repos/uuid/-/uuid-11.1.1.tgz`, HOST),
    ).toEqual({ name: "uuid", version: "11.1.1" });
  });

  test("a scoped tarball named by its full name yields the scoped name", () => {
    expect(
      parseTarballUrl(`${REGISTRY}/api/npm/npm-repos/@wix/web-bi-logger/-/@wix/web-bi-logger-2.1.24.tgz`, HOST),
    ).toEqual({ name: "@wix/web-bi-logger", version: "2.1.24" });
  });

  test("a scoped tarball named by its basename yields the scoped name", () => {
    expect(parseTarballUrl(`${REGISTRY}/@babel/core/-/core-7.29.0.tgz`, HOST)).toEqual({
      name: "@babel/core",
      version: "7.29.0",
    });
  });

  test("a url-encoded scope is decoded", () => {
    expect(parseTarballUrl(`${REGISTRY}/@babel%2fcore/-/core-7.29.0.tgz`, HOST)).toEqual({
      name: "@babel/core",
      version: "7.29.0",
    });
  });

  test("a prerelease version is kept whole", () => {
    expect(parseTarballUrl(`${REGISTRY}/react/-/react-19.0.0-rc.1.tgz`, HOST)).toEqual({
      name: "react",
      version: "19.0.0-rc.1",
    });
  });

  test("a tarball from another host or a packument is ignored", () => {
    expect(parseTarballUrl("https://registry.npmjs.org/uuid/-/uuid-11.1.1.tgz", HOST)).toBeNull();
    expect(parseTarballUrl(`${REGISTRY}/uuid`, HOST)).toBeNull();
  });
});

describe("the local package index", () => {
  test("finds packages cached by each package manager", () => {
    caches = new LocalCaches()
      .withBunPackage("react@19.2.4@@npm.example.com@@@1")
      .withBunPackage("@babel/core@7.29.0@@npm.example.com@@@1")
      .withNpmTarball(`${REGISTRY}/uuid/-/uuid-11.1.1.tgz`, "sha512-uuid")
      .withYarnPackage("@scope/thing", "1.0.0", `${REGISTRY}/@scope/thing/-/thing-1.0.0.tgz#sha1`, "sha512-thing");

    const index = caches.index();

    expect(index.get("react@19.2.4")).toEqual({ source: "bun" });
    expect(index.get("@babel/core@7.29.0")).toEqual({ source: "bun" });
    expect(index.get("uuid@11.1.1")).toEqual({ source: "npm", integrity: "sha512-uuid" });
    expect(index.get("@scope/thing@1.0.0")).toEqual({ source: "yarn", integrity: "sha512-thing" });
  });

  test("excludes packages cached from another registry", () => {
    caches = new LocalCaches()
      .withBunPackage("react@19.2.4@@other.example.com@@@1")
      .withNpmTarball("https://other.example.com/uuid/-/uuid-11.1.1.tgz", "sha512-uuid")
      .withYarnPackage("left-pad", "1.3.0", "https://other.example.com/left-pad/-/left-pad-1.3.0.tgz", "sha512-pad");

    expect(caches.index().size).toBe(0);
  });

  test("prefers a cache that records the integrity over bun's", () => {
    caches = new LocalCaches()
      .withBunPackage("uuid@11.1.1@@npm.example.com@@@1")
      .withNpmTarball(`${REGISTRY}/uuid/-/uuid-11.1.1.tgz`, "sha512-uuid");

    expect(caches.index().get("uuid@11.1.1")).toEqual({ source: "npm", integrity: "sha512-uuid" });
  });

  test("missing cache directories yield an empty index", () => {
    caches = new LocalCaches();
    expect(caches.index().size).toBe(0);
  });
});
