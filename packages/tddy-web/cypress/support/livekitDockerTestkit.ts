// Node.js only — imported by cypress.config.ts, not by browser-side support files.
//
// Mirrors the Rust LiveKitTestkit in packages/tddy-livekit-testkit:
// uses LIVEKIT_TESTKIT_WS_URL when set, otherwise starts an ephemeral Docker container.

import { execSync } from "child_process";

const LIVEKIT_IMAGE = "livekit/livekit-server:master";
const LIVEKIT_PORT = 7880;

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/**
 * Manages a LiveKit server for Cypress tests.
 *
 * When LIVEKIT_TESTKIT_WS_URL is set, connects to that existing instance.
 * Otherwise starts a Docker container with ephemeral host ports.
 *
 * Call `start()` to get the WS URL; call `stop()` in `after:run`.
 * `start()` is idempotent — multiple calls return the same URL.
 */
export class LivekitDockerTestkit {
  private containerName: string | null = null;
  private _startPromise: Promise<string> | null = null;

  start(): Promise<string> {
    if (!this._startPromise) {
      this._startPromise = this._doStart().catch((e) => {
        this._startPromise = null;
        throw e;
      });
    }
    return this._startPromise;
  }

  stop(): void {
    if (this.containerName) {
      try {
        execSync(`docker rm -f ${this.containerName}`, { stdio: "ignore" });
      } catch {
        /* ignore */
      }
      this.containerName = null;
    }
    this._startPromise = null;
  }

  private async _doStart(): Promise<string> {
    const envUrl = (process.env.LIVEKIT_TESTKIT_WS_URL ?? "").trim();
    if (envUrl) {
      await this._waitForReady(envUrl);
      return envUrl;
    }

    const name = `tddy-livekit-cypress-${process.pid}-${Date.now()}`;
    execSync(
      `docker run -d --name ${name}` +
        ` -p 0:${LIVEKIT_PORT} -p 0:7881 -p 0:7882/udp` +
        ` ${LIVEKIT_IMAGE} --dev --bind 0.0.0.0 --node-ip 127.0.0.1`,
      { stdio: "ignore" },
    );
    this.containerName = name;

    const port = await this._getPort(name);
    const wsUrl = `ws://127.0.0.1:${port}`;
    await this._waitForReady(wsUrl);
    return wsUrl;
  }

  private async _getPort(name: string): Promise<number> {
    for (let i = 0; i < 30; i++) {
      try {
        const out = execSync(`docker port ${name} ${LIVEKIT_PORT}`, {
          stdio: ["ignore", "pipe", "ignore"],
        })
          .toString()
          .trim();
        const port = parseInt(out.split(":").pop() ?? "", 10);
        if (!isNaN(port) && port > 0) return port;
      } catch {
        /* port not mapped yet */
      }
      await sleep(500);
    }
    throw new Error(
      `LiveKit container ${name}: port ${LIVEKIT_PORT} not mapped within 15s`,
    );
  }

  private async _waitForReady(wsUrl: string): Promise<void> {
    const httpUrl = wsUrl.replace(/^wss?:\/\//, "http://");
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline) {
      try {
        const r = await fetch(`${httpUrl}/`);
        if (r.ok) return;
      } catch {
        /* not ready yet */
      }
      await sleep(300);
    }
    throw new Error(`LiveKit at ${wsUrl} did not become ready within 20s`);
  }
}
