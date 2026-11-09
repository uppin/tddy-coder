/**
 * The `optimizeDeps` every web package's `vite.config.ts` uses, so Vite pre-bundles its dependencies
 * once at dev-server start instead of discovering one mid-run.
 *
 * Under Cypress component testing a dependency Vite meets for the first time while a spec is loading
 * makes it re-optimise and reload, and the spec in flight fails with "Failed to fetch dynamically
 * imported module …/cypress/support/component.ts" — a red shard that a re-run turns green. Scanning
 * the specs, the support files and the sources up front, and naming the heavy dependencies
 * explicitly, leaves nothing to discover.
 */

import { existsSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

/** Large or deeply imported dependencies; each is pre-bundled where the package can resolve it. */
export const PREBUNDLED = [
  "react",
  "react-dom",
  "react-dom/client",
  "react/jsx-runtime",
  "react/jsx-dev-runtime",
  "@bufbuild/protobuf",
  "@bufbuild/protobuf/wire",
  "@bufbuild/protobuf/codegenv2",
  "@connectrpc/connect",
  "@connectrpc/connect-web",
  "livekit-client",
  "lucide-react",
  "rxjs",
  "dequal",
  "debug",
  "@xterm/xterm",
  "@xterm/addon-fit",
  "@xterm/addon-unicode11",
  "@xterm/addon-webgl",
  "@radix-ui/react-select",
  "@radix-ui/react-scroll-area",
  "@radix-ui/react-tooltip",
  "@radix-ui/react-slot",
  "class-variance-authority",
  "clsx",
  "tailwind-merge",
  "cypress/react",
] as const;

/** `optimizeDeps` for the package rooted at `packageDir`. */
export function prebundledDeps(packageDir: string): { entries: string[]; include: string[] } {
  const resolvesFrom = (dir: string, name: string) => {
    try {
      createRequire(join(dir, "package.json")).resolve(name);
      return true;
    } catch {
      return false;
    }
  };
  // A workspace package this one depends on is served from source, so its own dependencies are
  // met only when a spec imports it — tddy-web reaches tddy-react-commons' rxjs and dequal, and
  // tddy-session-web's highlighter, that way. Its sources join the scan (below), and Vite's
  // nested form (`tddy-react-commons > rxjs`) pre-bundles the named heavy ones through it.
  const workspaceDirs = workspaceDependencies(packageDir)
    .map((name) => join(dirname(packageDir), name))
    .filter((dir) => existsSync(join(dir, "package.json")));
  const include = PREBUNDLED.flatMap((name) => {
    if (resolvesFrom(packageDir, name)) return [name];
    const via = workspaceDirs.find((dir) => resolvesFrom(dir, name));
    return via ? [`${packageName(via)} > ${name}`] : [];
  });
  return {
    entries: [
      ...(existsSync(join(packageDir, "index.html")) ? ["index.html"] : []),
      "src/**/*.{ts,tsx}",
      "cypress/component/**/*.cy.{ts,tsx}",
      "cypress/support/**/*.{ts,tsx}",
      // The sources of the workspace packages this one imports, so their dependencies are found
      // by the up-front scan rather than mid-spec.
      ...workspaceDirs.map((dir) => join(dir, "src/**/*.{ts,tsx}")),
      // bun tests import `bun:test`, which Vite cannot resolve; one such file aborts the whole scan,
      // and then every dependency is discovered mid-spec — the reload this helper exists to stop.
      "!**/*.test.{ts,tsx}",
    ],
    include,
  };
}

function readPackageJson(dir: string): {
  name?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
} {
  return JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
}

function packageName(dir: string): string {
  return readPackageJson(dir).name ?? dir;
}

/** Names of the `workspace:` dependencies (runtime and dev) of the package at `dir`. */
export function workspaceDependencies(dir: string): string[] {
  const pkg = readPackageJson(dir);
  return Object.entries({ ...pkg.dependencies, ...pkg.devDependencies })
    .filter(([, version]) => version.startsWith("workspace:"))
    .map(([name]) => name);
}
