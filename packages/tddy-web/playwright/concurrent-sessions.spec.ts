/**
 * Playwright E2E: Two browser windows connect to the same LiveKit room through
 * the real production app flow (GitHub stub auth → ConnectionForm → ConnectedTerminal
 * → GhosttyTerminalLiveKit).
 *
 * Each window must get its own independent VirtualTUI instance:
 *   1. Desktop (1280×720) must keep its terminal rendering unchanged when mobile joins.
 *   2. Mobile (375×667) must receive its own terminal output (not blank).
 *
 * Mirrors the gRPC-level test in grpc_concurrent_resize.rs but exercises the
 * full production LiveKit data-channel path.
 *
 * Requires:
 *   - LIVEKIT_TESTKIT_WS_URL (with matching internal/external ports for WebRTC ICE)
 *   - tddy-coder built (cargo build -p tddy-coder)
 *   - Web bundle built (bun run build in packages/tddy-web)
 */

import { test, expect } from "@playwright/test";
import { spawn, execSync, type ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "../../..");
const WEB_BUNDLE_PATH = path.resolve(__dirname, "../dist");

const DEV_API_KEY = "devkey";
const DEV_API_SECRET = "secret";
const SERVER_IDENTITY = "server";
const ROOM_NAME = `terminal-pw-${Date.now()}`;
const WEB_PORT = 8890;
const BASE_URL = `http://127.0.0.1:${WEB_PORT}`;

function getTddyCoderPath(): string {
  const release = path.join(REPO_ROOT, "target", "release", "tddy-coder");
  const debug = path.join(REPO_ROOT, "target", "debug", "tddy-coder");
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  throw new Error("tddy-coder binary not found. Run: cargo build -p tddy-coder");
}

function startTddyCoder(wsUrl: string): Promise<ChildProcess> {
  const binaryPath = getTddyCoderPath();
  if (!fs.existsSync(WEB_BUNDLE_PATH)) {
    throw new Error(`Web bundle not found at ${WEB_BUNDLE_PATH}. Run: bun run build`);
  }

  try {
    execSync(`fuser -k ${WEB_PORT}/tcp`, { stdio: "ignore" });
  } catch { /* port free or fuser unavailable */ }

  return new Promise((resolve, reject) => {
    const child = spawn(
      binaryPath,
      [
        "--daemon",
        "--grpc", "50055",
        "--livekit-url", wsUrl,
        "--livekit-api-key", DEV_API_KEY,
        "--livekit-api-secret", DEV_API_SECRET,
        "--livekit-room", ROOM_NAME,
        "--livekit-identity", SERVER_IDENTITY,
        "--web-port", String(WEB_PORT),
        "--web-host", "127.0.0.1",
        "--web-bundle-path", WEB_BUNDLE_PATH,
        "--github-stub",
        "--github-stub-codes=test-code:testuser",
      ],
      {
        stdio: ["ignore", "ignore", "ignore"],
        cwd: REPO_ROOT,
        env: { ...process.env, RUST_LOG: "info", TDDY_DISABLE_ANIMATIONS: "1" },
      },
    );

    const timeout = setTimeout(() => {
      stopProcess(child);
      reject(new Error("tddy-coder web server did not become ready within 15s"));
    }, 15_000);

    const interval = setInterval(() => {
      fetch(`${BASE_URL}/`)
        .then((r) => {
          if (r.ok) {
            clearInterval(interval);
            clearTimeout(timeout);
            resolve(child);
          }
        })
        .catch(() => {});
    }, 300);

    child.on("error", (err) => {
      clearInterval(interval);
      clearTimeout(timeout);
      reject(err);
    });
  });
}

function stopProcess(child: ChildProcess) {
  const pid = child.pid;
  if (pid == null) return;
  try {
    process.kill(-pid, "SIGTERM");
  } catch {
    try { child.kill("SIGTERM"); } catch { /* already dead */ }
  }
}

async function authenticateViaGitHubStub(page: import("@playwright/test").Page) {
  await page.goto(BASE_URL);
  await page.locator("[data-testid='github-login-button']").waitFor({ state: "visible", timeout: 10_000 });
  await page.locator("[data-testid='github-login-button']").click();
  await page.locator("[data-testid='livekit-url']").waitFor({ state: "visible", timeout: 20_000 });
}

async function fillAndConnect(
  page: import("@playwright/test").Page,
  wsUrl: string,
  identity: string,
) {
  await page.locator("[data-testid='livekit-url']").fill(wsUrl);
  await page.locator("[data-testid='livekit-identity']").fill(identity);
  await page.locator("[data-testid='livekit-room']").fill(ROOM_NAME);
  await page.locator("button[type='submit']").click();
}

async function waitForConnected(page: import("@playwright/test").Page) {
  await page.locator("[data-testid='connection-status-dot']").waitFor({ state: "visible", timeout: 25_000 });
  await expect(
    page.locator("[data-testid='connection-status-dot']"),
  ).toHaveAttribute("data-connection-status", "connected", { timeout: 25_000 });
}

test.describe("Concurrent LiveKit Sessions — Independent TUI Instances", () => {
  let wsUrl: string;
  let serverProcess: ChildProcess;

  test.beforeAll(async () => {
    wsUrl = process.env.LIVEKIT_TESTKIT_WS_URL ?? "";
    if (!wsUrl) {
      test.skip();
      return;
    }
    serverProcess = await startTddyCoder(wsUrl);
  });

  test.afterAll(async () => {
    if (serverProcess) stopProcess(serverProcess);
  });

  test("desktop terminal rendering unchanged after mobile connects", async ({ browser }) => {
    if (!wsUrl) test.skip();

    // Desktop connects and receives terminal output.
    const desktopCtx = await browser.newContext({ viewport: { width: 1280, height: 720 } });
    const desktop = await desktopCtx.newPage();
    desktop.on("console", (msg) => console.log(`[desktop] ${msg.type()}: ${msg.text()}`));

    await authenticateViaGitHubStub(desktop);
    await fillAndConnect(desktop, wsUrl, "desktop-pw");
    await waitForConnected(desktop);
    await desktop.locator("[data-testid='first-output-received']").waitFor({ state: "attached", timeout: 25_000 });

    // Let terminal rendering stabilize (TDDY_DISABLE_ANIMATIONS=1 freezes the spinner).
    await desktop.waitForTimeout(1_000);

    // Snapshot the desktop terminal before mobile joins.
    const screenshotBefore = await desktop.screenshot();

    // Mobile connects with a different identity and smaller viewport.
    const mobileCtx = await browser.newContext({ viewport: { width: 375, height: 667 } });
    const mobile = await mobileCtx.newPage();
    mobile.on("console", (msg) => console.log(`[mobile] ${msg.type()}: ${msg.text()}`));

    await authenticateViaGitHubStub(mobile);
    await fillAndConnect(mobile, wsUrl, "mobile-pw");
    await waitForConnected(mobile);

    // Wait for the mobile's resize OSC to propagate through the server.
    await desktop.waitForTimeout(3_000);

    // Desktop rendering must be pixel-identical — the mobile's smaller viewport
    // must not resize, reflow, or blank the desktop's independent TUI instance.
    const screenshotAfter = await desktop.screenshot();

    expect(
      screenshotBefore.equals(screenshotAfter),
      "Desktop terminal must not change when mobile connects with smaller viewport",
    ).toBe(true);

    await desktopCtx.close();
    await mobileCtx.close();
  });

  test("mobile window receives independent terminal content", async ({ browser }) => {
    if (!wsUrl) test.skip();

    // Desktop connects first (server needs at least one streamTerminalIO consumer).
    const desktopCtx = await browser.newContext({ viewport: { width: 1280, height: 720 } });
    const desktop = await desktopCtx.newPage();
    desktop.on("console", (msg) => console.log(`[desktop] ${msg.type()}: ${msg.text()}`));

    await authenticateViaGitHubStub(desktop);
    await fillAndConnect(desktop, wsUrl, "desktop-pw-2");
    await waitForConnected(desktop);
    await desktop.locator("[data-testid='first-output-received']").waitFor({ state: "attached", timeout: 25_000 });

    // Mobile connects — must get its own independent TUI stream.
    const mobileCtx = await browser.newContext({ viewport: { width: 375, height: 667 } });
    const mobile = await mobileCtx.newPage();
    mobile.on("console", (msg) => console.log(`[mobile] ${msg.type()}: ${msg.text()}`));

    await authenticateViaGitHubStub(mobile);
    await fillAndConnect(mobile, wsUrl, "mobile-pw-2");
    await waitForConnected(mobile);

    // Mobile must receive terminal output — not be a blank screen.
    // The first-output-received marker fires when streamTerminalIO sends data.
    await mobile.locator("[data-testid='first-output-received']").waitFor({ state: "attached", timeout: 25_000 });

    await desktopCtx.close();
    await mobileCtx.close();
  });
});
