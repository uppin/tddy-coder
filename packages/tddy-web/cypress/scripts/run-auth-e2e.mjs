#!/usr/bin/env node
/**
 * Start tddy-coder with --github-stub, wait for it to be ready,
 * then run the Cypress auth flow e2e test against it.
 */
import { spawn } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../../..");
const webPkg = path.resolve(__dirname, "../..");

const wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL;
if (!wsUrl) {
  console.error("LIVEKIT_TESTKIT_WS_URL must be set.");
  process.exit(1);
}

function findBinary() {
  const debug = path.join(repoRoot, "target/debug/tddy-coder");
  const release = path.join(repoRoot, "target/release/tddy-coder");
  // Prefer debug (more likely to be up-to-date during development)
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  console.error("tddy-coder binary not found. Run: cargo build -p tddy-coder");
  process.exit(1);
}

const binaryPath = findBinary();
const webBundlePath = path.join(webPkg, "dist");
if (!fs.existsSync(webBundlePath)) {
  console.error("Web bundle not found. Run: bun run build");
  process.exit(1);
}

const webPort = 8890;
const baseUrl = `http://127.0.0.1:${webPort}`;

// Start tddy-coder
const server = spawn(binaryPath, [
  "--daemon",
  "--livekit-url", wsUrl,
  "--livekit-api-key", "devkey",
  "--livekit-api-secret", "secret",
  "--livekit-room", "terminal-e2e",
  "--livekit-identity", "server",
  "--web-port", String(webPort),
  "--web-host", "127.0.0.1",
  "--web-bundle-path", webBundlePath,
  "--github-stub",
  "--github-stub-codes", "test-code:testuser",
], {
  stdio: ["ignore", "pipe", "pipe"],
  cwd: repoRoot,
  env: { ...process.env, RUST_LOG: "info" },
});

server.stdout?.on("data", (d) => process.stdout.write(d));
server.stderr?.on("data", (d) => process.stderr.write(d));

// Wait for server to be ready
async function waitForServer(url, timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const r = await fetch(url);
      if (r.ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`Server not ready at ${url} within ${timeoutMs}ms`);
}

try {
  await waitForServer(baseUrl);
  console.log(`Server ready at ${baseUrl}`);

  // Run Cypress
  const cypress = spawn("npx", [
    "cypress", "run", "--e2e",
    "--spec", "cypress/e2e/github-auth-flow.cy.ts",
    "--config", `baseUrl=${baseUrl},screenshotOnRunFailure=true`,
  ], {
    stdio: "inherit",
    cwd: webPkg,
    env: {
      ...process.env,
      ELECTRON_EXTRA_LAUNCH_ARGS: "--disable-gpu --no-sandbox",
      LIVEKIT_TESTKIT_WS_URL: wsUrl,
    },
  });

  const exitCode = await new Promise((resolve) => {
    cypress.on("close", resolve);
  });

  server.kill("SIGTERM");
  process.exit(exitCode);
} catch (err) {
  console.error(err);
  server.kill("SIGTERM");
  process.exit(1);
}
