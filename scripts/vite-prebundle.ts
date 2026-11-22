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

import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join } from "node:path";

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

/**
 * A dev-server plugin that names what escapes {@link prebundledDeps}: every dependency Vite meets
 * that the up-front pre-bundle did not cover, and every re-optimise-and-reload that follows.
 *
 * The reload is the failure: a spec loading at that moment dies with "Failed to fetch dynamically
 * imported module …/cypress/support/component.ts" before any test runs, and nothing in its output
 * says which dependency set it off. Every line this prints starts with `[vite-prebundle]`; the one
 * that matters is `[vite-prebundle] <package>: dependency discovered mid-run: <name>`, whose name
 * belongs in `PREBUNDLED` (or whose importer belongs in the scanned `entries`). Vite's own optimiser
 * messages are copied out too, so a scan that failed outright shows up as well.
 *
 * Lines go to stderr and, when `VITE_PREBUNDLE_LOG_DIR` is set (CI sets it), are appended to
 * `<dir>/<package>.log`, so they survive however the dev server's output is captured. It only
 * observes: nothing is retried, held back or failed because of it.
 */
export function prebundleDiagnostics(packageDir: string) {
  const pkg = basename(packageDir);
  const logDir = process.env.VITE_PREBUNDLE_LOG_DIR;
  const report = (line: string) => {
    const text = `[vite-prebundle] ${pkg}: ${line}`;
    process.stderr.write(`${text}\n`);
    if (!logDir) return;
    try {
      mkdirSync(logDir, { recursive: true });
      appendFileSync(join(logDir, `${pkg}.log`), `${new Date().toISOString()} ${text}\n`);
    } catch (error) {
      process.stderr.write(`[vite-prebundle] ${pkg}: cannot write ${logDir}: ${String(error)}\n`);
    }
  };
  /** Discovered mid-run since the last reload; named again by the reload they cause. */
  const sinceLastReload = new Set<string>();

  return {
    name: "tddy:vite-prebundle-diagnostics",
    apply: "serve" as const,

    configResolved(config: { logger: DiagnosedLogger }) {
      // Every environment's logger forwards to this one, so the optimiser's messages pass through.
      const logger = config.logger;
      for (const level of ["info", "warn", "error"] as const) {
        const original = logger[level].bind(logger);
        logger[level] = (msg: string, options?: never) => {
          const plain = String(msg).replace(ANSI, "");
          if (OPTIMISER_MESSAGE.test(plain)) {
            if (RELOADING.test(plain)) {
              const names = [...sinceLastReload].join(", ") || "none mid-run (see the lines above)";
              report(`re-optimised and reloading the page; dependencies discovered mid-run: ${names}`);
              sinceLastReload.clear();
            } else {
              // A failed scan lists its entries and the cause on the lines after the first.
              const lines = plain.split("\n").slice(0, level === "info" ? 1 : 12);
              report(`vite ${level}: ${lines.join("\n  ")}`);
            }
          }
          return original(msg, options);
        };
      }
    },

    configureServer(server: {
      environments?: Record<string, { depsOptimizer?: DiagnosedOptimizer }>;
    }) {
      const optimizer = server.environments?.client?.depsOptimizer;
      if (!optimizer) {
        report("no client dependency optimiser to watch (discovery is off); nothing can be discovered mid-run");
        return;
      }
      report("watching the dependency optimiser; a dependency the pre-bundle missed is named here");
      const register = optimizer.registerMissingImport.bind(optimizer);
      optimizer.registerMissingImport = (id: string, resolved: string) => {
        const { optimized, chunks, discovered } = optimizer.metadata;
        if (!optimized[id] && !chunks[id] && !discovered[id]) {
          // Before the first optimisation commits, the up-front scan may still report it and no
          // reload follows; after it, a newcomer always means a re-optimise.
          if (Object.keys(optimized).length === 0) {
            report(`dependency met before the first optimisation finished: ${id} (${resolved})`);
          } else {
            sinceLastReload.add(id);
            report(`dependency discovered mid-run: ${id} (${resolved})`);
          }
        }
        return register(id, resolved);
      };
    },
  };
}

/** The part of Vite's `Logger` {@link prebundleDiagnostics} wraps. */
type DiagnosedLogger = Record<"info" | "warn" | "error", (msg: string, options?: never) => void>;

/** The part of Vite's dependency optimiser {@link prebundleDiagnostics} reads and wraps. */
type DiagnosedOptimizer = {
  metadata: Record<"optimized" | "chunks" | "discovered", Record<string, unknown>>;
  registerMissingImport: (id: string, resolved: string) => unknown;
};

// biome-ignore lint/suspicious/noControlCharactersInRegex: strips terminal colours from Vite's messages
const ANSI = /\u001b\[[0-9;]*m/g;

const RELOADING = /optimized dependencies changed\. reloading/;

/** Vite's dependency-optimiser messages: discoveries, re-optimise reloads, scan and interop trouble. */
const OPTIMISER_MESSAGE =
  /new dependencies optimized|discovered while scanning|optimized dependencies changed|optimizeDeps|scan for dependencies|error while updating dependencies|Mixed ESM and CJS/i;

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
