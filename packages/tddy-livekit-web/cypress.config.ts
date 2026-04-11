import { defineConfig } from "cypress";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { spawn } from "child_process";
import { AccessToken } from "livekit-server-sdk";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const DEV_API_KEY = "devkey";
const DEV_API_SECRET = "secret";
const SERVER_IDENTITY = "server";
const CLIENT_IDENTITY = "client";
const ROOM_NAME = "echo-cypress-test";

function getEchoServerPath(): string {
  const repoRoot = path.resolve(__dirname, "../..");
  return path.join(repoRoot, "target", "debug", "examples", "echo_server");
}

export default defineConfig({
  env: {
    /** Forwarded for specs; transport tests skip when unset (see `transport.cy.tsx`). */
    LIVEKIT_TESTKIT_WS_URL: process.env.LIVEKIT_TESTKIT_WS_URL ?? "",
  },
  defaultCommandTimeout: 15000,
  component: {
    devServer: {
      framework: "react",
      bundler: "vite",
    },
    specPattern: "cypress/component/**/*.cy.{ts,tsx}",
    supportFile: "cypress/support/component.ts",
    setupNodeEvents(on, config) {
      const logFile = path.join(__dirname, "cypress-debug.log");
      let echoServerProcess: ReturnType<typeof spawn> | null = null;

      const stopEchoServer = () => {
        if (echoServerProcess) {
          echoServerProcess.kill("SIGTERM");
          echoServerProcess = null;
        }
      };

      on("after:run", () => {
        stopEchoServer();
      });

      on("task", {
        log(message: string) {
          console.log(message);
          fs.appendFileSync(logFile, message + "\n");
          return null;
        },
        async startEchoServer() {
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

          const binaryPath = getEchoServerPath();
          if (!fs.existsSync(binaryPath)) {
            throw new Error(
              `Echo server binary not found at ${binaryPath}. Run: cargo build -p tddy-livekit --example echo_server`
            );
          }

          return new Promise<{ url: string; roomName: string; clientToken: string }>(
            (resolve, reject) => {
              echoServerProcess = spawn(binaryPath, ["--url", wsUrl, "--token", token], {
                stdio: ["ignore", "pipe", "pipe"],
                env: { ...process.env, RUST_LOG: "info" },
              });

              let stdout = "";
              let resolved = false;
              echoServerProcess.stdout?.on("data", (data) => {
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
                      roomName: ROOM_NAME,
                      clientToken: clientJwt,
                    });
                  });
                }
              });

              echoServerProcess.stderr?.on("data", (data) => {
                const msg = data.toString();
                console.error("[echo_server]", msg);
                fs.appendFileSync(logFile, `[echo_server] ${msg}`);
              });

              echoServerProcess.on("error", reject);

              const timeout = setTimeout(() => {
                if (!resolved) {
                  resolved = true;
                  reject(new Error("Echo server did not print READY within 10s"));
                }
              }, 10000);
            }
          );
        },
        stopEchoServer() {
          stopEchoServer();
          return null;
        },
        async generateToken(args: { room: string; identity: string }) {
          const token = new AccessToken(DEV_API_KEY, DEV_API_SECRET, {
            identity: args.identity,
          });
          token.addGrant({
            roomJoin: true,
            room: args.room,
            canPublish: true,
            canSubscribe: true,
          });
          return token.toJwt();
        },
      });

      return config;
    },
  },
  video: false,
  screenshotOnRunFailure: false,
});
