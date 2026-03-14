import { defineConfig } from "cypress";
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

      on("after:run", () => {
        stopTerminalServer();
        stopTddyCoder();
      });

      on("task", {
        async startTerminalServer() {
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
            const cmd = [
              binaryPath,
              "--agent",
              "stub",
              "--prompt",
              "testfeature SKIP_QUESTIONS",
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

            let stdout = "";
            let resolved = false;
            terminalServerProcess.stdout?.on("data", (data) => {
              writeLog(data);
              stdout += data.toString();
              if (stdout.includes("READY") && !resolved) {
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
            });

            terminalServerProcess.stderr?.on("data", (data) => {
              writeLog(data);
              const s = data.toString();
              stdout += s;
              if (stdout.includes("READY") && !resolved) {
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
                reject(new Error("tddy-demo did not print READY within 15s"));
              }
            }, 15000);
          });
        },
        stopTerminalServer() {
          stopTerminalServer();
          return null;
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
      });
    },
  },
  video: false,
  screenshotOnRunFailure: false,
});
