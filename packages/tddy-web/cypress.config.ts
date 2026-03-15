import { defineConfig } from "cypress";
import { createRequire } from "module";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { fileURLToPath } from "url";
import { spawn } from "child_process";
import { AccessToken } from "livekit-server-sdk";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

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
  return fs.existsSync(release) ? release : debug;
}

function getWebBundlePath(): string {
  const pkgDir = path.resolve(__dirname, "..");
  return path.join(pkgDir, "dist");
}

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
    env: {
      LIVEKIT_TESTKIT_WS_URL: process.env.LIVEKIT_TESTKIT_WS_URL ?? "",
    },
    setupNodeEvents(on) {
      let terminalServerProcess: ReturnType<typeof spawn> | null = null;
      let tddyCoderProcess: ReturnType<typeof spawn> | null = null;
      let echoTerminalProcess: ReturnType<typeof spawn> | null = null;
      let authFlowProcess: ReturnType<typeof spawn> | null = null;

      const stopTerminalServer = () => {
        if (terminalServerProcess) {
          terminalServerProcess.kill("SIGTERM");
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

      on("after:run", () => {
        stopTerminalServer();
        stopTddyCoder();
        stopEchoTerminal();
        stopAuthFlow();
      });

      on("task", {
        async startTerminalServer(options?: { prompt?: string } | null) {
          const wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL;
          if (!wsUrl || wsUrl.trim() === "") {
            throw new Error(
              "LIVEKIT_TESTKIT_WS_URL must be set. Run ./run-livekit-testkit-server and export the URL."
            );
          }

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

        async startEchoTerminal() {
          const wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL;
          if (!wsUrl || wsUrl.trim() === "") {
            throw new Error(
              "LIVEKIT_TESTKIT_WS_URL must be set. Run ./run-livekit-testkit-server and export the URL."
            );
          }

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
              ["--url", wsUrl, "--token", token, "--room", ROOM_NAME],
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
                  room: ROOM_NAME,
                  canPublish: true,
                  canSubscribe: true,
                });
                clientToken.toJwt().then((clientJwt) => {
                  resolve({
                    url: wsUrl,
                    clientToken: clientJwt,
                    roomName: ROOM_NAME,
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
          const wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL;
          if (!wsUrl || wsUrl.trim() === "") {
            throw new Error(
              "LIVEKIT_TESTKIT_WS_URL must be set. Run ./run-livekit-testkit-server and export the URL."
            );
          }

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
              ],
              {
                stdio: ["ignore", "pipe", "pipe"],
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
          const wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL;
          if (!wsUrl || wsUrl.trim() === "") {
            throw new Error(
              "LIVEKIT_TESTKIT_WS_URL must be set. Run ./run-livekit-testkit-server and export the URL."
            );
          }

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
                "--github-stub-codes",
                "test-code:testuser",
              ],
              {
                stdio: ["ignore", "pipe", "pipe"],
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
      });
    },
  },
  video: false,
  screenshotOnRunFailure: false,
});
