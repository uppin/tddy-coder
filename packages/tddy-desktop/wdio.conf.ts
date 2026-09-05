/**
 * WebdriverIO configuration for the desktop end-to-end suite.
 *
 * The suite drives the **real** application: the compiled Tauri binary, its WKWebView, the
 * webview-IPC transport, and `tddy-daemon` running in the same process. That is the one layer no
 * other test covers — the Rust flavour is tested against a fake `FrameSink`, the browser transport
 * against an in-memory IPC double, and neither proves the two halves agree in a real window.
 *
 * On macOS this only works through the `embedded` driver provider: Apple ships no WebDriver for
 * WKWebView, so an external `tauri-driver` is Windows/Linux only. `embedded` runs the WebDriver
 * server *inside* the app, which is why the binary must be built with `--features wdio`
 * (`bun run e2e:build`). A default build has no such server and cannot be driven — deliberately,
 * since this application exists to have no listening socket.
 */

import { spawn } from "node:child_process";
import { createReadStream, existsSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { extname, join } from "node:path";

/** The workspace root, from this package. */
const REPO_ROOT = join(import.meta.dirname, "..", "..");

/**
 * The daemon configuration the suite runs against — no LiveKit, no Telegram, and file logging.
 * A run's evidence is in `tmp/logs/e2e-daemon`, since WebdriverIO owns the terminal.
 */
const E2E_DAEMON_CONFIG = join(import.meta.dirname, "e2e", "fixtures", "e2e.daemon.yaml");

/** The `--features wdio` debug binary that `e2e:build` produces. */
const APP_BINARY = join(REPO_ROOT, "target", "debug", "tddy-desktop");

/**
 * The origin the app loads its UI from.
 *
 * A debug build resolves `WebviewUrl::App` against `devUrl` rather than embedding the bundle, so
 * something must serve it. The port is not free to choose: Tauri grants capabilities per origin,
 * and a page served from anywhere other than the configured `devUrl` gets none — the IPC commands
 * would then be refused rather than merely unreachable, which is a confusing way to fail.
 */
const FRONTEND_PORT = 5173;

let frontend: Server | null = null;

export const config: WebdriverIO.Config = {
  runner: "local",
  framework: "mocha",
  specs: ["./e2e/**/*.e2e.ts"],
  maxInstances: 1,
  logLevel: "warn",
  reporters: ["spec"],
  mochaOpts: {
    // The app assembles the whole daemon before its window appears — a roster of 15 services, a
    // spawn-worker fork and a LiveKit dial. That is seconds, not milliseconds.
    timeout: 120_000,
  },
  services: [
    [
      "tauri",
      {
        appBinaryPath: APP_BINARY,
        driverProvider: "embedded",
        env: { TDDY_DAEMON_CONFIG: E2E_DAEMON_CONFIG, TDDY_WORKSPACE_ROOT: REPO_ROOT },
      },
    ],
  ],
  capabilities: [{}],

  /** Build the web bundle and serve it where the app expects to find it. */
  async onPrepare() {
    await run("bun", ["run", "--filter", "tddy-web", "build"], REPO_ROOT);
    frontend = await serveWebBundle();
  },

  async onComplete() {
    await new Promise<void>((resolve) => (frontend ? frontend.close(() => resolve()) : resolve()));
    frontend = null;
  },
};

const CONTENT_TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};

/**
 * Serve `packages/tddy-web/dist` on {@link FRONTEND_PORT}, in this process.
 *
 * Deliberately not a package: an on-demand `bunx` install is a second thing that can fail before
 * the app starts, and it mutates the lockfile. Unknown paths fall back to `index.html` because the
 * app routes on real pathnames (`/auth/callback` among them), exactly as its dev server does.
 */
function serveWebBundle(): Promise<Server> {
  const root = join(REPO_ROOT, "packages", "tddy-web", "dist");
  const server = createServer((request, response) => {
    const path = new URL(request.url ?? "/", "http://localhost").pathname;
    const candidate = join(root, path);
    const file = path !== "/" && existsSync(candidate) ? candidate : join(root, "index.html");
    response.writeHead(200, { "content-type": CONTENT_TYPES[extname(file)] ?? "application/octet-stream" });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(FRONTEND_PORT, "127.0.0.1", () => resolve(server));
  });
}

/** Run `command` to completion, failing the suite if it does not succeed. */
function run(command: string, args: string[], cwd: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) =>
      code === 0 ? resolve() : reject(new Error(`${command} ${args.join(" ")} exited with ${code}`)),
    );
  });
}
