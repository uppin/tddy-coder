// Node.js only — imported by cypress.config.ts, not by browser-side support files.
//
// Mirrors the Rust LiveKitTestkit in packages/tddy-livekit-testkit:
// uses LIVEKIT_TESTKIT_WS_URL when set, otherwise starts an ephemeral Docker container.

import { execSync } from "child_process";
import createDebug from "debug";

const LIVEKIT_IMAGE = "livekit/livekit-server:master";
const LIVEKIT_PORT = 7880;

/**
 * Diagnostics: `DEBUG=tddy:livekit-web:testkit` (or `tddy:livekit-web:*`) prints a line per phase to stderr, which
 * the Cypress run shows. Written to find where a start that normally takes seconds can run past the 60 s `cy.task`
 * timeout; a probe that logs its start and never its end is a hang.
 */
const log = createDebug("tddy:livekit-web:testkit");

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
      log(`using LIVEKIT_TESTKIT_WS_URL=${envUrl}`);
      await this._waitForReady(envUrl);
      return envUrl;
    }

    const name = `tddy-livekit-cypress-${process.pid}-${Date.now()}`;
    log(`docker run ${name} (blocking execSync)`);
    const ranAt = Date.now();
    execSync(
      `docker run -d --name ${name}` +
        ` -p 0:${LIVEKIT_PORT} -p 0:7881 -p 0:7882/udp` +
        ` ${LIVEKIT_IMAGE} --dev --bind 0.0.0.0 --node-ip 127.0.0.1`,
      // With diagnostics on, docker's own output (a pull, a daemon error) is part of the evidence.
      { stdio: log.enabled ? "inherit" : "ignore" },
    );
    this.containerName = name;
    log(`docker run returned after ${Date.now() - ranAt} ms`);

    const portAt = Date.now();
    const port = await this._getPort(name);
    log(`port ${LIVEKIT_PORT} mapped to ${port} after ${Date.now() - portAt} ms`);
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
        log(`docker port attempt ${i + 1}: unparseable output ${JSON.stringify(out)}`);
      } catch {
        log(`docker port attempt ${i + 1}: not mapped yet`);
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
    let attempt = 0;
    while (Date.now() < deadline) {
      attempt += 1;
      const probeAt = Date.now();
      // A probe that logs this line and never one of the lines below never returned.
      log(`ready probe ${attempt} -> ${httpUrl}/ started`);
      try {
        const r = await fetch(`${httpUrl}/`);
        log(`ready probe ${attempt} answered ${r.status} after ${Date.now() - probeAt} ms`);
        if (r.ok) return;
      } catch (e) {
        log(`ready probe ${attempt} failed after ${Date.now() - probeAt} ms: ${String(e)}`);
      }
      await sleep(300);
    }
    throw new Error(`LiveKit at ${wsUrl} did not become ready within 20s`);
  }
}
