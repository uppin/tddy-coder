/**
 * Acceptance tests for the transport this page reaches its own daemon with.
 *
 * One web bundle serves two hosts, so the same call must arrive at the daemon whether the page is
 * a browser tab talking to an HTTP server or a webview talking to the daemon in its own process —
 * carrying the same interceptor stack either way. A desktop operator silently losing the auth gate
 * would send tokens that expired while the window was open.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` (M7).
 */

import { afterEach, describe, expect, it, mock, spyOn } from "bun:test";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import {
  DaemonConfigService,
  GetConfigRequestSchema,
  GetConfigResponseSchema,
} from "../gen/daemon_config_pb";
import { createDefaultDaemonTransport } from "./daemonTransport";
import { TrafficMeterRegistry } from "./trafficMeter";
import { aBrowserPageServedFrom, aTauriHostedPage } from "../test-utils/daemonHosts";

/** The configuration the daemon under test is running with, as `GetConfig` answers it. */
const A_DAEMON_CONFIGURATION = { configPath: "/etc/tddy/daemon.yaml" };

/** A daemon answering `GetConfig` with {@link A_DAEMON_CONFIGURATION}. */
function aDaemonServingItsConfiguration(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(DaemonConfigService.method.getConfig, () =>
    create(GetConfigResponseSchema, A_DAEMON_CONFIGURATION),
  );
}

/** One request the HTTP daemon received: where it was addressed, and the token it carried. */
interface ReceivedHttpRequest {
  url: string;
  sessionToken: string;
}

/**
 * A daemon answering `GetConfig` over HTTP with {@link A_DAEMON_CONFIGURATION}, recording every
 * request it received. Stands in for the network, which is the boundary under test here.
 */
function anHttpDaemonServingItsConfiguration(): { received: () => ReceivedHttpRequest[] } {
  const received: ReceivedHttpRequest[] = [];
  const body = toBinary(GetConfigResponseSchema, create(GetConfigResponseSchema, A_DAEMON_CONFIGURATION));
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL, init?: RequestInit) => {
    received.push({
      url: String(url),
      sessionToken: fromBinary(GetConfigRequestSchema, init!.body as Uint8Array).sessionToken,
    });
    return new Response(body, { headers: { "content-type": "application/proto" } });
  });
  return { received: () => received };
}

/** An auth gate whose provider has installed a resolver handing out `token`. */
function anAuthGateHolding(token: string) {
  return { current: () => Promise.resolve(token) };
}

/** The two counters the control-plane meter accumulated, without its time-dependent rates. */
function bytesMeteredBy(registry: TrafficMeterRegistry): { bytesIn: number; bytesOut: number } {
  const { bytesIn, bytesOut } = registry.get("http").snapshot();
  return { bytesIn, bytesOut };
}

describe("the transport a page reaches its own daemon with", () => {
  afterEach(() => {
    mock.restore();
  });

  it("carries a call to the in-process daemon over the host application's IPC bridge inside the Tauri host", async () => {
    // Given — a page the desktop application loaded, hosting a daemon in its own process
    const daemon = aDaemonServingItsConfiguration();
    const transport = createDefaultDaemonTransport(
      undefined,
      undefined,
      aTauriHostedPage(DaemonConfigService, daemon.transport()),
    );

    // When — the page reads that daemon's configuration
    const response = await createClient(DaemonConfigService, transport).getConfig({
      sessionToken: "tok",
    });

    // Then — the call travelled the bridge and came back with what the daemon serves
    expect(response.configPath).toEqual("/etc/tddy/daemon.yaml");
    expect(daemon.callsTo(DaemonConfigService.method.getConfig).map((c) => c.sessionToken)).toEqual([
      "tok",
    ]);
  });

  it("posts a call to the daemon at the page's own origin in a plain browser", async () => {
    // Given — a page a standalone daemon served over HTTP
    const daemon = anHttpDaemonServingItsConfiguration();
    const transport = createDefaultDaemonTransport(
      undefined,
      undefined,
      aBrowserPageServedFrom("https://daemon.example"),
    );

    // When — the page reads that daemon's configuration
    const response = await createClient(DaemonConfigService, transport).getConfig({
      sessionToken: "tok",
    });

    // Then — the call went to the same origin the bundle came from, and came back
    expect(response.configPath).toEqual("/etc/tddy/daemon.yaml");
    expect(daemon.received().map((r) => r.url)).toEqual([
      "https://daemon.example/rpc/daemon_config.DaemonConfigService/GetConfig",
    ]);
  });

  it("fills in the auth gate's fresh session token on a call over the host application's IPC bridge", async () => {
    // Given — a desktop page whose auth provider has installed a token resolver
    const daemon = aDaemonServingItsConfiguration();
    const transport = createDefaultDaemonTransport(
      undefined,
      anAuthGateHolding("fresh-token"),
      aTauriHostedPage(DaemonConfigService, daemon.transport()),
    );

    // When — the page issues a call naming no token of its own
    await createClient(DaemonConfigService, transport).getConfig({});

    // Then — the gate put the current one on the request before it left
    expect(daemon.callsTo(DaemonConfigService.method.getConfig).map((c) => c.sessionToken)).toEqual([
      "fresh-token",
    ]);
  });

  it("fills in the auth gate's fresh session token on a call over HTTP", async () => {
    // Given — a browser page whose auth provider has installed a token resolver
    const daemon = anHttpDaemonServingItsConfiguration();
    const transport = createDefaultDaemonTransport(
      undefined,
      anAuthGateHolding("fresh-token"),
      aBrowserPageServedFrom("https://daemon.example"),
    );

    // When — the page issues a call naming no token of its own
    await createClient(DaemonConfigService, transport).getConfig({});

    // Then — the gate put the current one on the request before it left
    expect(daemon.received().map((r) => r.sessionToken)).toEqual(["fresh-token"]);
  });

  it("meters the bytes of a call over the host application's IPC bridge", async () => {
    // Given — a desktop page whose control-plane traffic is metered
    const registry = new TrafficMeterRegistry();
    const transport = createDefaultDaemonTransport(
      registry,
      undefined,
      aTauriHostedPage(DaemonConfigService, aDaemonServingItsConfiguration().transport()),
    );

    // When — the page reads the daemon's configuration
    await createClient(DaemonConfigService, transport).getConfig({ sessionToken: "tok" });

    // Then — both directions were counted. Out: `session_token: "tok"` — tag, length, 3 bytes.
    // In: `config_path: "/etc/tddy/daemon.yaml"` — tag, length, 21 bytes.
    expect(bytesMeteredBy(registry)).toEqual({ bytesOut: 5, bytesIn: 23 });
  });

  it("meters the bytes of a call over HTTP", async () => {
    // Given — a browser page whose control-plane traffic is metered
    const registry = new TrafficMeterRegistry();
    anHttpDaemonServingItsConfiguration();
    const transport = createDefaultDaemonTransport(
      registry,
      undefined,
      aBrowserPageServedFrom("https://daemon.example"),
    );

    // When — the page reads the daemon's configuration
    await createClient(DaemonConfigService, transport).getConfig({ sessionToken: "tok" });

    // Then — the same counters the desktop path produces, from the same interceptor
    expect(bytesMeteredBy(registry)).toEqual({ bytesOut: 5, bytesIn: 23 });
  });
});
