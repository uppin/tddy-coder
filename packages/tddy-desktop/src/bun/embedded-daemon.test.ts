import { describe, expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "fs";
import { join, resolve } from "path";
import { tmpdir } from "os";
import {
  DESKTOP_DEV_CONFIG_FILENAME,
  defaultDaemonBinaryPaths,
  loadRootDotEnv,
  monorepoRootFromBunDir,
  prepareDaemonConfigForSpawn,
  resolveDaemonBinaryPath,
  resolveDaemonConfigPath,
  resolveWorkspaceRoot,
} from "./embedded-daemon";

describe("resolveDaemonConfigPath", () => {
  test("returns null when unset and no default file", () => {
    const empty = mkdtempSync(join(tmpdir(), "tddy-desk-"));
    expect(resolveDaemonConfigPath({}, empty)).toBeNull();
  });

  test("returns dev.desktop.yaml at repo root when present", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-repo-"));
    const cfg = join(repo, DESKTOP_DEV_CONFIG_FILENAME);
    writeFileSync(cfg, "listen:\n  web_port: 1\n");
    expect(resolveDaemonConfigPath({}, repo)).toBe(cfg);
  });

  test("prefers TDDY_DAEMON_CONFIG over default file", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-repo-"));
    writeFileSync(join(repo, DESKTOP_DEV_CONFIG_FILENAME), "x: 1\n");
    expect(
      resolveDaemonConfigPath({ TDDY_DAEMON_CONFIG: "/custom.yaml" }, repo)
    ).toBe("/custom.yaml");
  });

  test("returns trimmed explicit path", () => {
    expect(
      resolveDaemonConfigPath({ TDDY_DAEMON_CONFIG: "  /tmp/d.yaml  " })
    ).toBe("/tmp/d.yaml");
  });
});

describe("loadRootDotEnv", () => {
  test("does not override existing env", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-env-"));
    writeFileSync(join(repo, ".env"), "FOO=fromfile\n");
    process.env.FOO = "preset";
    loadRootDotEnv(repo);
    expect(process.env.FOO).toBe("preset");
    delete process.env.FOO;
  });

  test("sets unset keys from .env", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-env-"));
    const key = `TDDY_TEST_DOTENV_${Date.now()}`;
    writeFileSync(join(repo, ".env"), `${key}=bar\n`);
    const prev = process.env[key];
    delete process.env[key];
    loadRootDotEnv(repo);
    expect(process.env[key]).toBe("bar");
    if (prev !== undefined) process.env[key] = prev;
    else delete process.env[key];
  });
});

describe("prepareDaemonConfigForSpawn", () => {
  test("returns same path when no CURRENT_USER placeholder", () => {
    const f = join(mkdtempSync(join(tmpdir(), "tddy-prep-")), "c.yaml");
    writeFileSync(f, "listen:\n  web_port: 8899\n");
    const r = prepareDaemonConfigForSpawn(f);
    expect(r.configPath).toBe(f);
    r.cleanup();
  });

  test("substitutes CURRENT_USER in temp file", () => {
    const f = join(mkdtempSync(join(tmpdir(), "tddy-prep-")), "c.yaml");
    writeFileSync(f, "users:\n  - os_user: CURRENT_USER\n");
    const prev = process.env.USER;
    process.env.USER = "testuser_subst";
    try {
      const r = prepareDaemonConfigForSpawn(f);
      expect(r.configPath).not.toBe(f);
      const out = readFileSync(r.configPath, "utf-8");
      expect(out).toContain("testuser_subst");
      expect(out).not.toContain("CURRENT_USER");
      r.cleanup();
      expect(existsSync(r.configPath)).toBe(false);
    } finally {
      if (prev !== undefined) process.env.USER = prev;
      else delete process.env.USER;
    }
  });
});

describe("resolveDaemonBinaryPath", () => {
  test("prefers TDDY_DAEMON_BINARY when file exists", () => {
    const dir = mkdtempSync(join(tmpdir(), "tddy-desk-"));
    const custom = join(dir, "my-daemon");
    writeFileSync(custom, "");
    const decoy = join(dir, "decoy");
    writeFileSync(decoy, "");
    const p = resolveDaemonBinaryPath(
      { TDDY_DAEMON_BINARY: custom },
      { bundled: decoy, release: decoy, debug: decoy }
    );
    expect(p).toBe(custom);
  });

  test("uses bundled when explicit missing and bundled exists", () => {
    const dir = mkdtempSync(join(tmpdir(), "tddy-desk-"));
    const bundled = join(dir, "bundled");
    writeFileSync(bundled, "");
    const p = resolveDaemonBinaryPath(
      {},
      { bundled, release: join(dir, "no-rel"), debug: join(dir, "no-dbg") }
    );
    expect(p).toBe(bundled);
  });

  test("falls back to release then debug", () => {
    const dir = mkdtempSync(join(tmpdir(), "tddy-desk-"));
    const rel = join(dir, "rel");
    writeFileSync(rel, "");
    expect(
      resolveDaemonBinaryPath(
        {},
        { bundled: join(dir, "no-b"), release: rel, debug: join(dir, "no-d") }
      )
    ).toBe(rel);
    const dbg = join(dir, "dbg");
    writeFileSync(dbg, "");
    expect(
      resolveDaemonBinaryPath(
        {},
        { bundled: join(dir, "no-b2"), release: join(dir, "no-r2"), debug: dbg }
      )
    ).toBe(dbg);
  });

  test("returns null when nothing exists", () => {
    const dir = mkdtempSync(join(tmpdir(), "tddy-desk-"));
    expect(
      resolveDaemonBinaryPath(
        {},
        {
          bundled: join(dir, "a"),
          release: join(dir, "b"),
          debug: join(dir, "c"),
        }
      )
    ).toBeNull();
  });
});

describe("defaultDaemonBinaryPaths", () => {
  test("paths are anchored at workspace root", () => {
    const repoRoot = mkdtempSync(join(tmpdir(), "tddy-ws-"));
    const paths = defaultDaemonBinaryPaths(repoRoot);
    expect(paths.bundled).toBe(
      join(repoRoot, "packages/tddy-desktop/resources/bin/tddy-daemon")
    );
    expect(paths.release).toBe(join(repoRoot, "target/release/tddy-daemon"));
    expect(paths.debug).toBe(join(repoRoot, "target/debug/tddy-daemon"));
  });
});

describe("resolveWorkspaceRoot", () => {
  test("TDDY_WORKSPACE_ROOT wins when directory exists", () => {
    const ws = mkdtempSync(join(tmpdir(), "tddy-ws-explicit-"));
    expect(resolveWorkspaceRoot("/bogus/none", { TDDY_WORKSPACE_ROOT: ws })).toBe(
      resolve(ws)
    );
  });

  test("walks up from cwd for dev.desktop.yaml", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-repo-yaml-"));
    writeFileSync(join(repo, DESKTOP_DEV_CONFIG_FILENAME), "x: 1\n");
    const nested = join(repo, "a", "b");
    mkdirSync(nested, { recursive: true });
    expect(resolveWorkspaceRoot("/x/y/z/bun", {}, nested)).toBe(resolve(repo));
  });

  test("walks up for Cargo.toml + packages/tddy-desktop/package.json", () => {
    const repo = mkdtempSync(join(tmpdir(), "tddy-repo-cargo-"));
    writeFileSync(join(repo, "Cargo.toml"), "[package]\nname = \"x\"\n");
    mkdirSync(join(repo, "packages", "tddy-desktop"), { recursive: true });
    writeFileSync(join(repo, "packages", "tddy-desktop", "package.json"), "{}");
    const nested = join(repo, "tools", "x");
    mkdirSync(nested, { recursive: true });
    expect(resolveWorkspaceRoot("/bun", {}, nested)).toBe(resolve(repo));
  });

  test("falls back to monorepoRootFromBunDir when no markers in cwd chain", () => {
    const isolated = mkdtempSync(join(tmpdir(), "tddy-iso-"));
    const nested = join(isolated, "nest");
    mkdirSync(nested, { recursive: true });
    const bunDir = join("/repo", "packages", "tddy-desktop", "src", "bun");
    expect(resolveWorkspaceRoot(bunDir, {}, nested)).toBe(resolve("/repo"));
  });
});

describe("monorepoRootFromBunDir", () => {
  test("four levels up from src/bun", () => {
    const bunDir = join("/repo", "packages", "tddy-desktop", "src", "bun");
    expect(monorepoRootFromBunDir(bunDir)).toBe("/repo");
  });
});
