import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { prebundleDiagnostics, prebundledDeps } from "./vite-prebundle";

const pkg = (name: string) => join(import.meta.dir, "..", "packages", name);

describe("prebundledDeps", () => {
  test("pre-bundles a dependency the package resolves itself by its plain name", () => {
    expect(prebundledDeps(pkg("tddy-react-commons")).include).toContain("rxjs");
  });

  test("pre-bundles a workspace dependency's own dependency through that workspace package", () => {
    const { include } = prebundledDeps(pkg("tddy-web"));

    expect(include).toContain("tddy-react-commons > rxjs");
    expect(include).toContain("tddy-react-commons > dequal");
  });

  test("scans the sources of the workspace packages it imports, so none of their dependencies is met mid-spec", () => {
    expect(prebundledDeps(pkg("tddy-web")).entries).toContain(join(pkg("tddy-session-web"), "src/**/*.{ts,tsx}"));
  });

  test("keeps bun tests out of the scan, because their bun:test import aborts it", () => {
    expect(prebundledDeps(pkg("tddy-web")).entries).toContain("!**/*.test.{ts,tsx}");
  });
});

describe("prebundleDiagnostics", () => {
  test("names a dependency met after the first optimisation as discovered mid-run", () => {
    const server = devServer();
    server.hasOptimised("react");

    server.imports("shiki", "/node_modules/shiki/dist/index.mjs");

    expect(server.reported()).toContain(
      "[vite-prebundle] tddy-session-web: dependency discovered mid-run: shiki (/node_modules/shiki/dist/index.mjs)",
    );
  });

  test("names the mid-run discoveries again on the re-optimise reload they cause", () => {
    const server = devServer();
    server.hasOptimised("react");
    server.imports("shiki");

    server.viteLogs("info", "\u001b[32m✨ optimized dependencies changed. reloading\u001b[39m");

    expect(server.reported()).toContain(
      "[vite-prebundle] tddy-session-web: re-optimised and reloading the page; dependencies discovered mid-run: shiki",
    );
  });

  test("keeps a dependency met during cold start out of the reload line, because the scan may still cover it", () => {
    const server = devServer();
    server.imports("react");
    server.hasOptimised("react");

    server.viteLogs("info", "✨ optimized dependencies changed. reloading");

    expect(server.reported()).toContain(
      "[vite-prebundle] tddy-session-web: dependency met before the first optimisation finished: react (/resolved/react)",
    );
    expect(server.reported()).toContain("dependencies discovered mid-run: none mid-run (see the lines above)");
  });

  test("stays quiet about a dependency the pre-bundle already covers", () => {
    const server = devServer();
    server.hasOptimised("react");

    server.imports("react");

    expect(server.reported()).not.toContain("react");
  });

  test("copies a failed scan with the lines that name its cause", () => {
    const server = devServer();

    server.viteLogs("error", "(!) Failed to scan for dependencies from entries:\n  src/a.tsx\n  cause");

    expect(server.reported()).toContain(
      "[vite-prebundle] tddy-session-web: vite error: (!) Failed to scan for dependencies from entries:\n    src/a.tsx\n    cause",
    );
  });

  test("leaves Vite's other messages alone and still hands every message to Vite", () => {
    const server = devServer();

    server.viteLogs("info", "page reload src/App.tsx");

    expect(server.reported()).not.toContain("page reload");
    expect(server.passedToVite()).toEqual(["page reload src/App.tsx"]);
  });
});

/** A dev server with `prebundleDiagnostics` installed over a stand-in logger and optimiser. */
function devServer() {
  const logDir = mkdtempSync(join(tmpdir(), "vite-prebundle-"));
  const previous = process.env.VITE_PREBUNDLE_LOG_DIR;
  process.env.VITE_PREBUNDLE_LOG_DIR = logDir;
  const plugin = prebundleDiagnostics(pkg("tddy-session-web"));
  process.env.VITE_PREBUNDLE_LOG_DIR = previous;

  const seenByVite: string[] = [];
  const record = (msg: string) => void seenByVite.push(msg);
  const logger = { info: record, warn: record, error: record };
  const metadata = {
    optimized: {} as Record<string, unknown>,
    chunks: {} as Record<string, unknown>,
    discovered: {} as Record<string, unknown>,
  };
  const optimizer = {
    metadata,
    registerMissingImport: (id: string) => {
      metadata.discovered[id] = {};
      return id;
    },
  };
  plugin.configResolved({ logger });
  plugin.configureServer({ environments: { client: { depsOptimizer: optimizer } } });

  return {
    hasOptimised(...ids: string[]) {
      for (const id of ids) {
        metadata.optimized[id] = {};
        delete metadata.discovered[id];
      }
    },
    imports(id: string, resolved = `/resolved/${id}`) {
      optimizer.registerMissingImport(id, resolved);
    },
    viteLogs(level: "info" | "warn" | "error", msg: string) {
      logger[level](msg);
    },
    reported() {
      const file = join(logDir, "tddy-session-web.log");
      return existsSync(file) ? readFileSync(file, "utf8").replace(/^\S+ /gm, "") : "";
    },
    passedToVite: () => seenByVite,
  };
}
