/**
 * ConnectRPC integration tests for EchoService.Echo (unary).
 *
 * Auto-starts tddy-coder with web server if not already running.
 * Set CONNECTRPC_BASE_URL=http://127.0.0.1:PORT/rpc to use an existing server.
 */

import { createConnectTransport } from "@connectrpc/connect-node";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { EchoService, EchoRequestSchema } from "../gen/test/echo_service_pb";
import { spawn, type ChildProcess } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../../..");

function findBinary(): string {
  const debug = path.join(repoRoot, "target/debug/tddy-coder");
  const release = path.join(repoRoot, "target/release/tddy-coder");
  if (fs.existsSync(debug)) return debug;
  if (fs.existsSync(release)) return release;
  throw new Error("tddy-coder binary not found. Run: cargo build -p tddy-coder");
}

function findWebBundle(): string {
  const dist = path.join(repoRoot, "packages/tddy-web/dist");
  if (fs.existsSync(dist)) return dist;
  throw new Error("Web bundle not found. Run: bun run build");
}

const PORT = 8887;
const baseUrl = process.env.CONNECTRPC_BASE_URL ?? `http://127.0.0.1:${PORT}/rpc`;
const externalServer = !!process.env.CONNECTRPC_BASE_URL;

let serverProcess: ChildProcess | null = null;

async function waitForServer(url: string, timeoutMs = 10000): Promise<void> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const r = await fetch(url.replace("/rpc", "/"));
      if (r.ok) return;
    } catch {}
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`Server not ready at ${url} within ${timeoutMs}ms`);
}

beforeAll(async () => {
  if (externalServer) return;

  const binaryPath = findBinary();
  const webBundlePath = findWebBundle();

  serverProcess = spawn(binaryPath, [
    "--daemon",
    "--web-port", String(PORT),
    "--web-host", "127.0.0.1",
    "--web-bundle-path", webBundlePath,
  ], {
    stdio: ["ignore", "pipe", "pipe"],
    cwd: repoRoot,
    env: { ...process.env, RUST_LOG: "warn" },
  });

  serverProcess.stderr?.on("data", () => {});
  serverProcess.stdout?.on("data", () => {});

  await waitForServer(baseUrl);
});

afterAll(() => {
  if (serverProcess) {
    serverProcess.kill("SIGTERM");
    serverProcess = null;
  }
});

const transport = createConnectTransport({
  baseUrl,
  httpVersion: "1.1",
  useBinaryFormat: true,
});

const client = createClient(EchoService, transport);

describe("EchoService.Echo (unary)", () => {
  test("echoes message", async () => {
    const res = await client.echo(create(EchoRequestSchema, { message: "hello" }));
    expect(res.message).toBe("hello");
    expect(typeof res.timestamp).toBe("bigint");
  });
});
