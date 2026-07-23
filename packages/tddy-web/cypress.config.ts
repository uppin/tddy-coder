import { defineConfig } from "cypress";
import { createRequire } from "module";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { fileURLToPath } from "url";
import { execSync, spawn } from "child_process";
import * as net from "net";
import { AccessToken } from "livekit-server-sdk";

/** Ask the OS for a free TCP port (bind :0, read it, release). Avoids hardcoded-port collisions. */
async function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      const port = typeof addr === "object" && addr ? addr.port : 0;
      srv.close(() => resolve(port));
    });
  });
}
import { LivekitDockerTestkit } from "./cypress/support/livekitDockerTestkit.js";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import {
  ExchangeCodeRequestSchema,
  ExchangeCodeResponseSchema,
  GetAuthUrlRequestSchema,
  GetAuthUrlResponseSchema,
} from "./src/gen/auth_pb.js";
import {
  StartSessionRequestSchema,
  StartSessionResponseSchema,
} from "./src/gen/connection_pb.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Read by `vite.config.ts` when Cypress spawns the component-testing dev server (script env alone is not always inherited). */
process.env.CYPRESS_DISABLE_REACT_COMPILER = "1";

const DEV_API_KEY = "devkey";
const DEV_API_SECRET = "secret";
const SERVER_IDENTITY = "server";
const CLIENT_IDENTITY = "client";
const ROOM_NAME = "terminal-e2e";

function getTddyDemoPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  return path.join(repoRoot, "target", "debug", "tddy-demo");
}

function getEchoTerminalPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  return path.join(repoRoot, "target", "debug", "examples", "echo_terminal");
}

function getTddyCoderPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  const release = path.join(repoRoot, "target", "release", "tddy-coder");
  const debug = path.join(repoRoot, "target", "debug", "tddy-coder");
  // Prefer debug: it is what `cargo build -p tddy-coder` updates; stale release can miss RPC
  // services (e.g. auth.AuthService) and break Connect e2e.
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  return debug;
}

function getWebBundlePath(): string {
  // cypress.config.ts lives in packages/tddy-web; production bundle is tddy-web/dist.
  return path.join(__dirname, "dist");
}

function getTddyDaemonPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  const debug = path.join(repoRoot, "target", "debug", "tddy-daemon");
  const release = path.join(repoRoot, "target", "release", "tddy-daemon");
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  return debug;
}

function getTddyDemoTuiPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  return path.join(repoRoot, "target", "debug", "tddy-demo-tui");
}

const testkit = new LivekitDockerTestkit();

export default defineConfig({
  component: {
    devServer: {
      framework: "react",
      bundler: "vite",
    },
    specPattern: "cypress/component/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/component.ts",
  },
  e2e: {
    baseUrl: process.env.CYPRESS_BASE_URL ?? "http://localhost:6006",
    specPattern: "cypress/e2e/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/e2e.ts",
    async setupNodeEvents(on, config) {
      // Start LiveKit eagerly so Cypress.env("LIVEKIT_TESTKIT_WS_URL") is populated
      // before any spec's before() hook runs. Tests skip when the var is empty; this
      // ensures they always run (with a container auto-started if not pre-set).
      const livekitWsUrl = await testkit.start();
      config.env = { ...config.env, LIVEKIT_TESTKIT_WS_URL: livekitWsUrl };

      // Headless Electron/Chromium hangs at launch in sandboxed/container environments unless the
      // OS sandbox and GPU are disabled and /dev/shm is not used. `ELECTRON_EXTRA_LAUNCH_ARGS` does
      // not apply to Cypress's own browser — flags must be added via this hook.
      on("before:browser:launch", (browser, launchOptions) => {
        if (browser.family === "chromium" || browser.name === "electron") {
          launchOptions.args.push("--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage");
        }
        return launchOptions;
      });

      let terminalServerProcess: ReturnType<typeof spawn> | null = null;
      let tddyCoderProcess: ReturnType<typeof spawn> | null = null;
      let echoTerminalProcess: ReturnType<typeof spawn> | null = null;
      let authFlowProcess: ReturnType<typeof spawn> | null = null;
      let daemonProcess: ReturnType<typeof spawn> | null = null;
      let daemonConfigPath: string | null = null;
      let daemonE2eWorkDir: string | null = null;

      /** On POSIX, `script` + shell leaves a child process group; signal the whole group so LiveKit sees the server leave. */
      const stopSpawnedProcessTree = (child: ReturnType<typeof spawn>) => {
        const pid = child.pid;
        if (pid == null) return;
        if (process.platform === "win32") {
          child.kill("SIGTERM");
          return;
        }
        try {
          process.kill(-pid, "SIGTERM");
        } catch {
          try {
            child.kill("SIGTERM");
          } catch {
            /* ignore */
          }
        }
      };

      const stopTerminalServer = () => {
        if (terminalServerProcess) {
          stopSpawnedProcessTree(terminalServerProcess);
          terminalServerProcess = null;
        }
      };

      const stopTddyCoder = () => {
        if (tddyCoderProcess) {
          tddyCoderProcess.kill("SIGTERM");
          tddyCoderProcess = null;
        }
      };

      const stopEchoTerminal = () => {
        if (echoTerminalProcess) {
          echoTerminalProcess.kill("SIGTERM");
          echoTerminalProcess = null;
        }
      };

      const stopAuthFlow = () => {
        if (authFlowProcess) {
          authFlowProcess.kill("SIGTERM");
          authFlowProcess = null;
        }
      };

      const stopDaemon = () => {
        if (daemonProcess) {
          daemonProcess.kill("SIGTERM");
          daemonProcess = null;
        }
        if (daemonConfigPath && fs.existsSync(daemonConfigPath)) {
          try { fs.unlinkSync(daemonConfigPath); } catch { /* ignore */ }
          daemonConfigPath = null;
        }
        if (daemonE2eWorkDir && fs.existsSync(daemonE2eWorkDir)) {
          try { fs.rmSync(daemonE2eWorkDir, { recursive: true, force: true }); } catch { /* ignore */ }
          daemonE2eWorkDir = null;
        }
      };

      on("after:run", () => {
        stopTerminalServer();
        stopTddyCoder();
        stopEchoTerminal();
        stopAuthFlow();
        stopDaemon();
        testkit.stop();
      });

      on("task", {
        async startTerminalServer(options?: { prompt?: string } | null) {
          const wsUrl = await testkit.start();

          const serverToken = new AccessToken(DEV_API_KEY, DEV_API_SECRET, {
            identity: SERVER_IDENTITY,
          });
          serverToken.addGrant({
            roomJoin: true,
            room: ROOM_NAME,
            canPublish: true,
            canSubscribe: true,
          });
          const token = await serverToken.toJwt();

          const binaryPath = getTddyDemoPath();
          if (!fs.existsSync(binaryPath)) {
            throw new Error(
              `tddy-demo binary not found at ${binaryPath}. Run: cargo build -p tddy-demo`
            );
          }

          return new Promise<{
            url: string;
            clientToken: string;
            roomName: string;
            serverLogPath?: string;
          }>((resolve, reject) => {
            const logDir = process.env.TMPDIR ?? os.tmpdir();
            const logPath = path.join(logDir, `tddy-demo-e2e-${Date.now()}.log`);
            const logStream = fs.createWriteStream(logPath, { flags: "a" });

            const repoRoot = path.resolve(__dirname, "../..");
            const prompt = options?.prompt ?? "testfeature SKIP_QUESTIONS";
            const cmd = [
              binaryPath,
              "--agent",
              "stub",
              "--prompt",
              prompt,
              "--livekit-url",
              wsUrl,
              "--livekit-token",
              token,
              "--livekit-room",
              ROOM_NAME,
              "--livekit-identity",
              SERVER_IDENTITY,
            ]
              .map((s) => (s.includes(" ") || s.includes("'") ? `'${s.replace(/'/g, "'\\''")}'` : s))
              .join(" ");
            const args = ["-q", "-c", cmd, "-"];
            const scriptCmd = "script";
            terminalServerProcess = spawn(scriptCmd, args, {
              stdio: ["ignore", "pipe", "pipe"],
              cwd: repoRoot,
              detached: process.platform !== "win32",
              env: { ...process.env, RUST_LOG: "debug" },
            });

            const writeLog = (data: string | Buffer) => {
              logStream.write(data);
            };

            let output = "";
            let resolved = false;

            // tddy-demo enters TUI mode via `script` (PTY). It never prints "READY" —
            // it renders ratatui ANSI frames. Detect readiness by looking for either:
            // - EnterAlternateScreen (\x1b[?1049h) — TUI is rendering
            // - Enough output bytes (>200) — the initial frame has been sent
            // - "Session:" in stderr — workflow has started
            const checkReady = () => {
              const hasTuiOutput = output.includes("\x1b[?1049h") || output.length > 200;
              const hasSession = output.includes("Session:");
              if ((hasTuiOutput || hasSession) && !resolved) {
                resolved = true;
                clearTimeout(timeout);
                const clientToken = new AccessToken(DEV_API_KEY, DEV_API_SECRET, {
                  identity: CLIENT_IDENTITY,
                });
                clientToken.addGrant({
                  roomJoin: true,
                  room: ROOM_NAME,
                  canPublish: true,
                  canSubscribe: true,
                });
                clientToken.toJwt().then((clientJwt) => {
                  resolve({
                    url: wsUrl,
                    clientToken: clientJwt,
                    roomName: ROOM_NAME,
                    serverLogPath: logPath,
                  });
                });
              }
            };

            terminalServerProcess.stdout?.on("data", (data) => {
              writeLog(data);
              output += data.toString();
              checkReady();
            });

            terminalServerProcess.stderr?.on("data", (data) => {
              writeLog(data);
              output += data.toString();
              checkReady();
            });

            terminalServerProcess.on("error", (err) => {
              if (!resolved) {
                resolved = true;
                reject(err);
              }
            });

            const timeout = setTimeout(() => {
              if (!resolved) {
                resolved = true;
                terminalServerProcess?.kill("SIGTERM");
                terminalServerProcess = null;
                reject(new Error(`tddy-demo did not become ready within 15s (output: ${output.length} bytes)`));
              }
            }, 15000);
          });
        },
        stopTerminalServer() {
          stopTerminalServer();
          return null;
        },

        async startEchoTerminal(options?: { roomName?: string } | null) {
          const wsUrl = await testkit.start();

          const room = options?.roomName ?? ROOM_NAME;

          const serverToken = new AccessToken(DEV_API_KEY, DEV_API_SECRET, {
            identity: SERVER_IDENTITY,
          });
          serverToken.addGrant({
            roomJoin: true,
            room,
            canPublish: true,
            canSubscribe: true,
          });
          const token = await serverToken.toJwt();

          const binaryPath = getEchoTerminalPath();
          if (!fs.existsSync(binaryPath)) {
            throw new Error(
              `echo_terminal binary not found at ${binaryPath}. Run: cargo build -p tddy-livekit --example echo_terminal`
            );
          }

          return new Promise<{
            url: string;
            clientToken: string;
            roomName: string;
            echoTerminalLogPath?: string;
          }>((resolve, reject) => {
            const logDir = process.env.TMPDIR ?? os.tmpdir();
            const echoLogPath = path.join(
              logDir,
              `echo_terminal-e2e-${Date.now()}.log`
            );
            const echoLogStream = fs.createWriteStream(echoLogPath, {
              flags: "a",
            });

            const repoRoot = path.resolve(__dirname, "../..");
            echoTerminalProcess = spawn(
              binaryPath,
              ["--url", wsUrl, "--token", token, "--room", room],
              {
                stdio: ["ignore", "pipe", "pipe"],
                cwd: repoRoot,
                env: { ...process.env, RUST_LOG: "info" },
              }
            );

            let output = "";
            let resolved = false;

            const writeEchoLog = (data: string | Buffer) => {
              echoLogStream.write(data);
            };
            const timeout = setTimeout(() => {
              if (!resolved) {
                resolved = true;
                stopEchoTerminal();
                reject(new Error("echo_terminal did not print READY within 15s"));
              }
            }, 15000);

            const checkReady = () => {
              if (output.includes("READY") && !resolved) {
                resolved = true;
                clearTimeout(timeout);
                const clientToken = new AccessToken(DEV_API_KEY, DEV_API_SECRET, {
                  identity: CLIENT_IDENTITY,
                });
                clientToken.addGrant({
                  roomJoin: true,
                  room,
                  canPublish: true,
                  canSubscribe: true,
                });
                clientToken.toJwt().then((clientJwt) => {
                  resolve({
                    url: wsUrl,
                    clientToken: clientJwt,
                    roomName: room,
                    echoTerminalLogPath: echoLogPath,
                  });
                });
              }
            };

            echoTerminalProcess.stdout?.on("data", (data) => {
              writeEchoLog(data);
              output += data.toString();
              checkReady();
            });
            echoTerminalProcess.stderr?.on("data", (data) => {
              writeEchoLog(data);
              output += data.toString();
              checkReady();
            });
            echoTerminalProcess.on("error", (err) => {
              if (!resolved) {
                resolved = true;
                clearTimeout(timeout);
                reject(err);
              }
            });
          });
        },

        stopEchoTerminal() {
          stopEchoTerminal();
          return null;
        },

        log(message: string) {
          console.log(message);
          return null;
        },

        readLogFile(filePath: string): string {
          try {
            return fs.readFileSync(filePath, "utf-8");
          } catch {
            return `(could not read ${filePath})`;
          }
        },

        async ocrScreenshot(imagePath: string): Promise<string> {
          const require = createRequire(import.meta.url);
          const { createWorker } = require("tesseract.js");
          const pkgDir = __dirname;
          const fullPath = path.isAbsolute(imagePath)
            ? imagePath
            : path.join(pkgDir, imagePath);
          if (!fs.existsSync(fullPath)) {
            throw new Error(`OCR: image not found at ${fullPath}`);
          }
          const worker = await createWorker("eng");
          try {
            const {
              data: { text },
            } = await worker.recognize(fullPath);
            return text ?? "";
          } finally {
            await worker.terminate();
          }
        },

        async startTddyCoderForConnectFlow() {
          const wsUrl = await testkit.start();

          const binaryPath = getTddyCoderPath();
          if (!fs.existsSync(binaryPath)) {
            throw new Error(
              `tddy-coder binary not found. Run: cargo build -p tddy-coder`
            );
          }

          const webBundlePath = getWebBundlePath();
          if (!fs.existsSync(webBundlePath)) {
            throw new Error(
              `Web bundle not found at ${webBundlePath}. Run: bun run build`
            );
          }

          const webPort = 8889;
          const baseUrl = `http://127.0.0.1:${webPort}`;

          stopTddyCoder();
          if (process.platform !== "win32") {
            try {
              execSync(`fuser -k ${webPort}/tcp`, { stdio: "ignore" });
            } catch {
              /* port free or fuser unavailable */
            }
          }

          return new Promise<{ baseUrl: string }>((resolve, reject) => {
            const repoRoot = path.resolve(__dirname, "../..");
            tddyCoderProcess = spawn(
              binaryPath,
              [
                "--daemon",
                "--livekit-url",
                wsUrl,
                "--livekit-api-key",
                DEV_API_KEY,
                "--livekit-api-secret",
                DEV_API_SECRET,
                "--livekit-room",
                ROOM_NAME,
                "--livekit-identity",
                SERVER_IDENTITY,
                "--web-port",
                String(webPort),
                "--web-host",
                "127.0.0.1",
                "--web-bundle-path",
                webBundlePath,
                "--github-stub",
                // Single argv so the ':' in test-code:testuser is never split by a shell layer.
                "--github-stub-codes=test-code:testuser",
              ],
              {
                // Drain is required if piping: a full stderr buffer can block the daemon before
                // the web server finishes starting.
                stdio: ["ignore", "ignore", "ignore"],
                cwd: repoRoot,
                env: { ...process.env, RUST_LOG: "info" },
              }
            );

            const timeout = setTimeout(() => {
              stopTddyCoder();
              reject(new Error("tddy-coder web server did not become ready within 15s"));
            }, 15000);

            const interval = setInterval(() => {
              fetch(`${baseUrl}/`)
                .then((r) => {
                  if (r.ok) {
                    clearInterval(interval);
                    clearTimeout(timeout);
                    resolve({ baseUrl });
                  }
                })
                .catch(() => {});
            }, 300);

            tddyCoderProcess.on("error", (err) => {
              clearInterval(interval);
              clearTimeout(timeout);
              reject(err);
            });
          });
        },

        stopTddyCoderForConnectFlow() {
          stopTddyCoder();
          return null;
        },

        async startTddyCoderForAuthFlow() {
          const wsUrl = await testkit.start();

          const binaryPath = getTddyCoderPath();
          if (!fs.existsSync(binaryPath)) {
            throw new Error(
              `tddy-coder binary not found. Run: cargo build -p tddy-coder`
            );
          }

          const webBundlePath = getWebBundlePath();
          if (!fs.existsSync(webBundlePath)) {
            throw new Error(
              `Web bundle not found at ${webBundlePath}. Run: bun run build`
            );
          }

          const webPort = 8890;
          const baseUrl = `http://127.0.0.1:${webPort}`;

          stopAuthFlow();
          if (process.platform !== "win32") {
            try {
              execSync(`fuser -k ${webPort}/tcp`, { stdio: "ignore" });
            } catch {
              /* port free or fuser unavailable */
            }
          }

          return new Promise<{ baseUrl: string }>((resolve, reject) => {
            const repoRoot = path.resolve(__dirname, "../..");
            authFlowProcess = spawn(
              binaryPath,
              [
                "--daemon",
                "--livekit-url",
                wsUrl,
                "--livekit-api-key",
                DEV_API_KEY,
                "--livekit-api-secret",
                DEV_API_SECRET,
                "--livekit-room",
                ROOM_NAME,
                "--livekit-identity",
                SERVER_IDENTITY,
                "--web-port",
                String(webPort),
                "--web-host",
                "127.0.0.1",
                "--web-bundle-path",
                webBundlePath,
                "--github-stub",
                "--github-stub-codes=test-code:testuser",
              ],
              {
                stdio: ["ignore", "ignore", "ignore"],
                cwd: repoRoot,
                env: { ...process.env, RUST_LOG: "info" },
              }
            );

            const timeout = setTimeout(() => {
              stopAuthFlow();
              reject(new Error("tddy-coder (auth flow) did not become ready within 15s"));
            }, 15000);

            const interval = setInterval(() => {
              fetch(`${baseUrl}/`)
                .then((r) => {
                  if (r.ok) {
                    clearInterval(interval);
                    clearTimeout(timeout);
                    resolve({ baseUrl });
                  }
                })
                .catch(() => {});
            }, 300);

            authFlowProcess.on("error", (err) => {
              clearInterval(interval);
              clearTimeout(timeout);
              reject(err);
            });
          });
        },

        stopTddyCoderForAuthFlow() {
          stopAuthFlow();
          return null;
        },

        async startDaemonWithDemoTui(_options?: unknown) {
          const demoTuiPath = getTddyDemoTuiPath();
          if (!fs.existsSync(demoTuiPath)) {
            throw new Error(
              `tddy-demo-tui binary not found at ${demoTuiPath}. Run: cargo build -p tddy-demo-tui`,
            );
          }
          const daemonBinPath = getTddyDaemonPath();
          if (!fs.existsSync(daemonBinPath)) {
            throw new Error(
              `tddy-daemon binary not found at ${daemonBinPath}. Run: cargo build -p tddy-daemon`,
            );
          }
          const webBundlePath = getWebBundlePath();
          if (!fs.existsSync(webBundlePath)) {
            throw new Error(
              `Web bundle not found at ${webBundlePath}. Run: bun run build`,
            );
          }

          const webPort = 8891;
          const baseUrl = `http://127.0.0.1:${webPort}`;

          stopDaemon();
          if (process.platform !== "win32") {
            try {
              execSync(`fuser -k ${webPort}/tcp`, { stdio: "ignore" });
            } catch { /* port free */ }
          }

          // Create an isolated work directory for this test run so we do not pollute
          // the user's real ~/.tddy or the repository.
          const workDir = path.join(os.tmpdir(), `tddy-e2e-demo-tui-${Date.now()}`);
          fs.mkdirSync(workDir, { recursive: true });
          daemonE2eWorkDir = workDir;

          // Set up a minimal git project the daemon can use for claude-cli sessions:
          //   1. a bare repo (acts as "remote" so git fetch origin works)
          //   2. a clone of that bare repo (the main_repo_path shown in the UI)
          const bareRepoPath = path.join(workDir, "bare.git");
          const mainRepoPath = path.join(workDir, "main");
          execSync(`git init --bare "${bareRepoPath}"`);
          execSync(`git clone "file://${bareRepoPath}" "${mainRepoPath}"`);
          execSync(`git -C "${mainRepoPath}" commit --allow-empty -m "e2e init"`, {
            env: { ...process.env, GIT_AUTHOR_NAME: "e2e", GIT_AUTHOR_EMAIL: "e2e@e2e",
                   GIT_COMMITTER_NAME: "e2e", GIT_COMMITTER_EMAIL: "e2e@e2e" },
          });
          execSync(`git -C "${mainRepoPath}" push origin HEAD:main`);

          // Write projects.yaml so the daemon finds one project.
          const projectsDir = path.join(workDir, "projects");
          fs.mkdirSync(projectsDir, { recursive: true });
          const projectId = `e2e-demo-tui-project`;
          const projectsYaml = [
            "projects:",
            `- project_id: "${projectId}"`,
            `  name: "E2E Demo TUI Project"`,
            `  git_url: "file://${bareRepoPath}"`,
            `  main_repo_path: "${mainRepoPath}"`,
          ].join("\n") + "\n";
          fs.writeFileSync(path.join(projectsDir, "projects.yaml"), projectsYaml);

          const osUser = os.userInfo().username;
          const configContent = [
            `listen:`,
            `  web_port: ${webPort}`,
            `  web_host: 127.0.0.1`,
            `web_bundle_path: ${webBundlePath}`,
            // Session-token signing reads `livekit.api_secret`; without it the stub OAuth
            // ExchangeCode fails with "session token signing is not configured".
            `livekit:`,
            `  api_key: ${DEV_API_KEY}`,
            `  api_secret: ${DEV_API_SECRET}`,
            `github:`,
            `  stub: true`,
            `  stub_codes: "test-code:testuser"`,
            `users:`,
            `  - github_user: "testuser"`,
            `    os_user: "${osUser}"`,
            `claude_cli:`,
            `  binary_path: ${demoTuiPath}`,
          ].join("\n");

          const configFile = path.join(workDir, "daemon.yaml");
          fs.writeFileSync(configFile, configContent);
          daemonConfigPath = configFile;

          const repoRoot = path.resolve(__dirname, "../..");
          return new Promise<{ baseUrl: string }>((resolve, reject) => {
            daemonProcess = spawn(daemonBinPath, ["--config", configFile], {
              stdio: ["ignore", "ignore", "ignore"],
              cwd: repoRoot,
              env: { ...process.env, RUST_LOG: "info", TDDY_PROJECTS_DIR: projectsDir },
            });

            const timeout = setTimeout(() => {
              stopDaemon();
              reject(new Error("tddy-daemon did not become ready within 20s"));
            }, 20000);

            const interval = setInterval(() => {
              fetch(`${baseUrl}/`)
                .then((r) => {
                  if (r.ok) {
                    clearInterval(interval);
                    clearTimeout(timeout);
                    resolve({ baseUrl });
                  }
                })
                .catch(() => {});
            }, 300);

            daemonProcess.on("error", (err) => {
              clearInterval(interval);
              clearTimeout(timeout);
              reject(err);
            });
          });
        },

        stopDaemonWithDemoTui() {
          stopDaemon();
          return null;
        },

        async getTestSessionToken({ baseUrl }: { baseUrl: string }): Promise<string> {
          // Two-step OAuth stub flow: GetAuthUrl (generates+stores state), then ExchangeCode.
          // ConnectRPC unary endpoints use Content-Type: application/proto (raw binary protobuf).
          async function rpc<Req, Res>(
            method: string,
            reqSchema: Parameters<typeof toBinary>[0],
            reqMsg: Parameters<typeof toBinary>[1],
            resSchema: Parameters<typeof fromBinary>[0],
          ): Promise<Res> {
            const body = toBinary(reqSchema, reqMsg);
            const res = await fetch(`${baseUrl}/rpc/${method}`, {
              method: "POST",
              headers: { "Content-Type": "application/proto", "Connect-Protocol-Version": "1" },
              body,
            });
            if (!res.ok) {
              const text = await res.text().catch(() => "(no body)");
              throw new Error(`${method} HTTP ${res.status}: ${text}`);
            }
            return fromBinary(resSchema, new Uint8Array(await res.arrayBuffer())) as Res;
          }

          // Step 1: get auth URL — this generates and stores the OAuth state server-side.
          const authUrlResp = await rpc<never, { state: string }>(
            "auth.AuthService/GetAuthUrl",
            GetAuthUrlRequestSchema,
            create(GetAuthUrlRequestSchema, {}),
            GetAuthUrlResponseSchema,
          );
          const state = (authUrlResp as { state: string }).state;

          // Step 2: exchange stub code + stored state for a session token.
          const exchangeResp = await rpc<never, { sessionToken: string }>(
            "auth.AuthService/ExchangeCode",
            ExchangeCodeRequestSchema,
            create(ExchangeCodeRequestSchema, { code: "test-code", state }),
            ExchangeCodeResponseSchema,
          );
          const token = (exchangeResp as { sessionToken: string }).sessionToken;
          if (!token) throw new Error("ExchangeCode returned no sessionToken");
          return token;
        },

        // Start a real tddy-daemon wired to the LiveKit testkit so recipe sessions attach over
        // LiveKit (connectSession → connected-livekit), which the PR-Stack Chat presenter requires.
        // Uses agent "stub" (== tddy-demo's backend) for deterministic agent output.
        async startDaemonForPrStack(): Promise<{
          baseUrl: string;
          projectId: string;
          toolPath: string;
        }> {
          const daemonBinPath = getTddyDaemonPath();
          if (!fs.existsSync(daemonBinPath)) {
            throw new Error(`tddy-daemon not found at ${daemonBinPath}. Run: cargo build -p tddy-daemon`);
          }
          const toolPath = getTddyCoderPath();
          if (!fs.existsSync(toolPath)) {
            throw new Error(`tddy-coder not found at ${toolPath}. Run: cargo build -p tddy-coder`);
          }
          const webBundlePath = getWebBundlePath();
          if (!fs.existsSync(webBundlePath)) {
            throw new Error(`Web bundle not found at ${webBundlePath}. Run: bun run build`);
          }

          const webPort = await getFreePort();
          const baseUrl = `http://127.0.0.1:${webPort}`;

          stopDaemon();

          const workDir = path.join(os.tmpdir(), `tddy-e2e-prstack-${Date.now()}`);
          fs.mkdirSync(workDir, { recursive: true });
          daemonE2eWorkDir = workDir;

          const bareRepoPath = path.join(workDir, "bare.git");
          const mainRepoPath = path.join(workDir, "main");
          execSync(`git init --bare "${bareRepoPath}"`);
          execSync(`git clone "file://${bareRepoPath}" "${mainRepoPath}"`);
          execSync(`git -C "${mainRepoPath}" commit --allow-empty -m "e2e init"`, {
            env: { ...process.env, GIT_AUTHOR_NAME: "e2e", GIT_AUTHOR_EMAIL: "e2e@e2e",
                   GIT_COMMITTER_NAME: "e2e", GIT_COMMITTER_EMAIL: "e2e@e2e" },
          });
          execSync(`git -C "${mainRepoPath}" push origin HEAD:main`);

          const projectsDir = path.join(workDir, "projects");
          fs.mkdirSync(projectsDir, { recursive: true });
          const projectId = `e2e-prstack-project`;
          const projectsYaml =
            [
              "projects:",
              `- project_id: "${projectId}"`,
              `  name: "E2E PR-Stack Project"`,
              `  git_url: "file://${bareRepoPath}"`,
              `  main_repo_path: "${mainRepoPath}"`,
            ].join("\n") + "\n";
          fs.writeFileSync(path.join(projectsDir, "projects.yaml"), projectsYaml);

          const osUser = os.userInfo().username;
          const configContent = [
            `listen:`,
            `  web_port: ${webPort}`,
            `  web_host: 127.0.0.1`,
            `web_bundle_path: ${webBundlePath}`,
            `daemon_instance_id: e2e`,
            `livekit:`,
            `  url: ${livekitWsUrl}`,
            `  api_key: ${DEV_API_KEY}`,
            `  api_secret: ${DEV_API_SECRET}`,
            `  public_url: ${livekitWsUrl}`,
            `  common_room: tddy-lobby`,
            `github:`,
            `  stub: true`,
            `  stub_codes: "test-code:testuser"`,
            `users:`,
            `  - github_user: "testuser"`,
            `    os_user: "${osUser}"`,
            `allowed_agents:`,
            `  - id: stub`,
            `    label: "Stub"`,
            `allowed_tools:`,
            `  - path: "${toolPath}"`,
            `    label: "tddy-coder (debug)"`,
          ].join("\n");

          const configFile = path.join(workDir, "daemon.yaml");
          fs.writeFileSync(configFile, configContent);
          daemonConfigPath = configFile;

          const repoRoot = path.resolve(__dirname, "../..");
          return new Promise((resolve, reject) => {
            daemonProcess = spawn(daemonBinPath, ["--config", configFile], {
              stdio: ["ignore", "ignore", "ignore"],
              cwd: repoRoot,
              env: { ...process.env, RUST_LOG: "info", TDDY_PROJECTS_DIR: projectsDir },
            });
            const timeout = setTimeout(() => {
              stopDaemon();
              reject(new Error("tddy-daemon (pr-stack) did not become ready within 20s"));
            }, 20000);
            const interval = setInterval(() => {
              fetch(`${baseUrl}/`)
                .then((r) => {
                  if (r.ok) {
                    clearInterval(interval);
                    clearTimeout(timeout);
                    resolve({ baseUrl, projectId, toolPath });
                  }
                })
                .catch(() => {});
            }, 300);
            daemonProcess.on("error", (err) => {
              clearInterval(interval);
              clearTimeout(timeout);
              reject(err);
            });
          });
        },

        stopDaemonForPrStack() {
          stopDaemon();
          return null;
        },

        // Start a pr-stack orchestrator session over the daemon via raw Connect-RPC. Deterministic
        // (no UI create flow). Returns the new session id.
        async startPrStackSession({
          baseUrl,
          sessionToken,
          projectId,
          toolPath,
        }: {
          baseUrl: string;
          sessionToken: string;
          projectId: string;
          toolPath: string;
        }): Promise<string> {
          const body = toBinary(
            StartSessionRequestSchema,
            create(StartSessionRequestSchema, {
              sessionToken,
              projectId,
              toolPath,
              agent: "stub",
              recipe: "pr-stack",
              sessionType: "",
              branchWorktreeIntent: "new_branch_from_base",
              newBranchName: `e2e-pr-stack-${Date.now()}`,
            }),
          );
          const res = await fetch(`${baseUrl}/rpc/connection.ConnectionService/StartSession`, {
            method: "POST",
            headers: { "Content-Type": "application/proto", "Connect-Protocol-Version": "1" },
            body,
          });
          if (!res.ok) {
            const text = await res.text().catch(() => "(no body)");
            throw new Error(`StartSession HTTP ${res.status}: ${text}`);
          }
          const resp = fromBinary(
            StartSessionResponseSchema,
            new Uint8Array(await res.arrayBuffer()),
          ) as { sessionId: string };
          if (!resp.sessionId) throw new Error("StartSession returned no sessionId");
          return resp.sessionId;
        },

        // Live breadcrumb log written unbuffered to a fixed file so progress is visible during a run
        // even when Cypress's stdout is block-buffered. Tail /tmp/tddy-e2e-live.log while running.
        e2elog(msg: string) {
          const repoRoot = path.resolve(__dirname, "../..");
          const logPath = path.join(repoRoot, "tmp", "e2e-live.log");
          try {
            fs.mkdirSync(path.dirname(logPath), { recursive: true });
            fs.appendFileSync(logPath, `${new Date().toISOString()} ${msg}\n`);
          } catch (e) {
            // eslint-disable-next-line no-console
            console.log("[e2e] log-write-failed", String(e));
          }
          // eslint-disable-next-line no-console
          console.log("[e2e]", msg);
          return null;
        },
      });

      return config;
    },
  },
  video: false,
  screenshotOnRunFailure: false,
});
