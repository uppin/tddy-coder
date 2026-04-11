import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "fs";
import { join, resolve } from "path";
import { tmpdir, userInfo } from "os";

export type DaemonBinaryPaths = {
  bundled: string;
  release: string;
  debug: string;
};

/** Repo-root filename used when `TDDY_DAEMON_CONFIG` is unset (desktop dev). */
export const DESKTOP_DEV_CONFIG_FILENAME = "dev.desktop.yaml";

let daemonProc: ReturnType<typeof Bun.spawn> | null = null;
let preparedConfigCleanup: (() => void) | null = null;

/** Monorepo root when this file still lives under `packages/tddy-desktop/src/bun` (source / some bundles). */
export function monorepoRootFromBunDir(bunDir: string): string {
  return join(bunDir, "../../../..");
}

/**
 * Resolve tddy-coder repo root for config, `cwd`, and `target/*` binaries.
 *
 * 1. `TDDY_WORKSPACE_ROOT` if set and directory exists.
 * 2. Walk upward from `cwd` for `dev.desktop.yaml` or `Cargo.toml` + `packages/tddy-desktop/package.json`.
 * 3. Fall back to four levels up from `bunDir` (works when `import.meta.dir` is still under the package).
 */
export function resolveWorkspaceRoot(
  bunDir: string,
  env: Record<string, string | undefined> = process.env,
  cwd: string = process.cwd()
): string {
  const explicit = env.TDDY_WORKSPACE_ROOT?.trim();
  if (explicit && existsSync(explicit)) {
    return resolve(explicit);
  }
  let d = resolve(cwd);
  for (let i = 0; i < 20; i++) {
    if (existsSync(join(d, DESKTOP_DEV_CONFIG_FILENAME))) {
      return d;
    }
    if (
      existsSync(join(d, "Cargo.toml")) &&
      existsSync(join(d, "packages/tddy-desktop/package.json"))
    ) {
      return d;
    }
    const parent = join(d, "..");
    if (parent === d) {
      break;
    }
    d = parent;
  }
  return resolve(monorepoRootFromBunDir(bunDir));
}

/**
 * Load repo-root `.env` without overriding existing `process.env` entries
 * (same rule as `./web-dev`).
 */
export function loadRootDotEnv(repoRoot: string): void {
  const path = join(repoRoot, ".env");
  if (!existsSync(path)) {
    return;
  }
  const text = readFileSync(path, "utf-8");
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }
    const eq = trimmed.indexOf("=");
    if (eq <= 0) {
      continue;
    }
    const key = trimmed.slice(0, eq).trim();
    let value = trimmed.slice(eq + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    if (process.env[key] === undefined) {
      process.env[key] = value;
    }
  }
}

/**
 * Resolve daemon YAML: `TDDY_DAEMON_CONFIG`, else `repoRoot/dev.desktop.yaml` if that file exists.
 */
export function resolveDaemonConfigPath(
  env: Record<string, string | undefined> = process.env,
  repoRoot?: string | null
): string | null {
  const explicit = env.TDDY_DAEMON_CONFIG?.trim();
  if (explicit) {
    return explicit;
  }
  if (repoRoot) {
    const desktopDefault = join(repoRoot, DESKTOP_DEV_CONFIG_FILENAME);
    if (existsSync(desktopDefault)) {
      return desktopDefault;
    }
  }
  return null;
}

/**
 * If the YAML contains `CURRENT_USER`, write a temp copy with substitution (same as ./web-dev).
 * Otherwise use the source path as-is.
 */
export function prepareDaemonConfigForSpawn(sourcePath: string): {
  configPath: string;
  cleanup: () => void;
} {
  const raw = readFileSync(sourcePath, "utf-8");
  if (!raw.includes("CURRENT_USER")) {
    return {
      configPath: sourcePath,
      cleanup: () => {},
    };
  }
  const user =
    process.env.USER ||
    process.env.USERNAME ||
    userInfo().username ||
    "";
  const replaced = raw.split("CURRENT_USER").join(user);
  const dir = mkdtempSync(join(tmpdir(), "tddy-desktop-daemon-"));
  const out = join(dir, "config.yaml");
  writeFileSync(out, replaced);
  return {
    configPath: out,
    cleanup: () => {
      try {
        rmSync(dir, { recursive: true, force: true });
      } catch {
        /* ignore */
      }
    },
  };
}

/** Resolve tddy-daemon binary: TDDY_DAEMON_BINARY, then bundled, then workspace target/{release,debug}. */
export function resolveDaemonBinaryPath(
  env: Record<string, string | undefined>,
  paths: DaemonBinaryPaths
): string | null {
  const explicit = env.TDDY_DAEMON_BINARY?.trim();
  if (explicit && existsSync(explicit)) {
    return explicit;
  }
  if (existsSync(paths.bundled)) {
    return paths.bundled;
  }
  if (existsSync(paths.release)) {
    return paths.release;
  }
  if (existsSync(paths.debug)) {
    return paths.debug;
  }
  return null;
}

/** Binary paths under the resolved workspace root (Electrobun dev cwd is inside `.app`, not the repo). */
export function defaultDaemonBinaryPaths(repoRoot: string): DaemonBinaryPaths {
  return {
    bundled: join(
      repoRoot,
      "packages/tddy-desktop/resources/bin/tddy-daemon"
    ),
    release: join(repoRoot, "target/release/tddy-daemon"),
    debug: join(repoRoot, "target/debug/tddy-daemon"),
  };
}

export function getEmbeddedDaemonPid(): number | null {
  return daemonProc?.pid ?? null;
}

function clearPreparedConfig(): void {
  if (preparedConfigCleanup) {
    preparedConfigCleanup();
    preparedConfigCleanup = null;
  }
}

export function stopEmbeddedDaemon(): void {
  clearPreparedConfig();
  const p = daemonProc;
  if (!p) {
    return;
  }
  try {
    p.kill();
  } catch {
    /* ignore */
  }
  daemonProc = null;
}

export function startEmbeddedDaemon(bunDir: string = import.meta.dir): void {
  if (daemonProc !== null) {
    return;
  }
  const repoRoot = resolveWorkspaceRoot(bunDir);
  loadRootDotEnv(repoRoot);

  const sourceConfig = resolveDaemonConfigPath(process.env, repoRoot);
  const binary = resolveDaemonBinaryPath(
    process.env,
    defaultDaemonBinaryPaths(repoRoot)
  );
  if (!sourceConfig) {
    console.error(
      `[tddy-desktop] Embedded tddy-daemon skipped: set TDDY_DAEMON_CONFIG or add ${DESKTOP_DEV_CONFIG_FILENAME} at the repo root.`
    );
    return;
  }
  if (!binary) {
    console.error(
      "[tddy-desktop] Embedded tddy-daemon skipped: binary not found. Run `cargo build -p tddy-daemon` from the repo root, `bun run build-daemon` in packages/tddy-desktop, or set TDDY_DAEMON_BINARY."
    );
    return;
  }

  const { configPath, cleanup } = prepareDaemonConfigForSpawn(sourceConfig);
  preparedConfigCleanup = cleanup;

  try {
    daemonProc = Bun.spawn([binary, "--config", configPath], {
      cwd: repoRoot,
      stdout: "inherit",
      stderr: "inherit",
      stdin: "ignore",
      env: process.env,
    });
    console.info(
      `[tddy-desktop] Started embedded tddy-daemon (pid ${daemonProc.pid}, workspace ${repoRoot}, config ${sourceConfig}, binary ${binary})`
    );
  } catch (e) {
    console.error("[tddy-desktop] Failed to spawn tddy-daemon:", e);
    clearPreparedConfig();
    daemonProc = null;
  }
}

export function registerEmbeddedDaemonCleanup(): void {
  const stop = () => stopEmbeddedDaemon();
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  process.on("beforeExit", stop);
}
