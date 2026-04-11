/**
 * Ensures embedded `tddy-daemon` can resolve the monorepo when Electrobun's cwd / import paths
 * differ from a repo-root launch (see `scripts/desktop-dev.sh`).
 */
import { existsSync } from "fs";
import { dirname, join } from "path";

const scriptsDir = import.meta.dir;
const pkgDir = dirname(scriptsDir);
const root = join(pkgDir, "..", "..");

if (!process.env.TDDY_WORKSPACE_ROOT?.trim()) {
  process.env.TDDY_WORKSPACE_ROOT = root;
}

const desktopYaml = join(root, "dev.desktop.yaml");
if (!process.env.TDDY_DAEMON_CONFIG?.trim() && existsSync(desktopYaml)) {
  process.env.TDDY_DAEMON_CONFIG = desktopYaml;
}

if (!process.env.TDDY_DAEMON_BINARY?.trim()) {
  const debugBin = join(root, "target/debug/tddy-daemon");
  const releaseBin = join(root, "target/release/tddy-daemon");
  if (existsSync(debugBin)) {
    process.env.TDDY_DAEMON_BINARY = debugBin;
  } else if (existsSync(releaseBin)) {
    process.env.TDDY_DAEMON_BINARY = releaseBin;
  }
}

const proc = Bun.spawn(["electrobun", "dev", ...Bun.argv.slice(2)], {
  cwd: pkgDir,
  env: process.env,
  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
});
process.exit(await proc.exited);
