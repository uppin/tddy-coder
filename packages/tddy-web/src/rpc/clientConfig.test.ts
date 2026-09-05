/**
 * Acceptance tests for where the web bundle reads its startup configuration from.
 *
 * The browser dashboard must keep making exactly the `GET /api/config` request the daemon has
 * always served beside the bundle. A page inside the Tauri desktop application has no HTTP origin
 * to make it against, and asks the same daemon for the same payload over RPC instead.
 *
 * See `docs/dev/1-WIP/2026-09-04-tauri-desktop-single-process.md` (M7).
 */

import { afterEach, describe, expect, it, mock, spyOn } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { DaemonConfigService, GetClientConfigResponseSchema } from "../gen/daemon_config_pb";
import { createDefaultDaemonTransport } from "./daemonTransport";
import { loadClientConfig } from "./clientConfig";
import { aBrowserPageServedFrom, aTauriHostedPage } from "../test-utils/daemonHosts";

/** A daemon in daemon mode, with a LiveKit block and one allowed agent. */
function aDaemonServingItsClientConfig(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(DaemonConfigService.method.getClientConfig, () =>
    create(GetClientConfigResponseSchema, {
      livekitUrl: "ws://127.0.0.1:7880",
      commonRoom: "tddy-lobby",
      daemonMode: true,
      daemonInstanceId: "udoo",
      allowedAgents: [{ id: "claude", label: "Claude" }],
      debug: "tddy:rpc:*",
    }),
  );
}

/** The URLs a page fetched over HTTP while the test ran. */
interface HttpEndpoint {
  fetched: () => string[];
}

/** A daemon serving `/api/config` with the JSON payload its web server has always served. */
function anHttpDaemonServingItsClientConfig(): HttpEndpoint {
  const fetched: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL) => {
    fetched.push(String(url));
    return Response.json({
      livekit_url: "ws://127.0.0.1:7880",
      common_room: "tddy-lobby",
      daemon_mode: true,
      daemon_instance_id: "udoo",
      allowed_agents: [{ id: "claude", label: "Claude" }],
      debug: "tddy:rpc:*",
    });
  });
  return { fetched: () => fetched };
}

/** A daemon whose web server has no configuration to serve. */
function anHttpDaemonServingNoClientConfig(): HttpEndpoint {
  const fetched: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL) => {
    fetched.push(String(url));
    return new Response("not found", { status: 404 });
  });
  return { fetched: () => fetched };
}

/** The configuration both sources are read into, so one assertion covers both paths. */
const THE_DAEMONS_CLIENT_CONFIG = {
  livekitUrl: "ws://127.0.0.1:7880",
  livekitRoom: undefined,
  commonRoom: "tddy-lobby",
  daemonMode: true,
  daemonInstanceId: "udoo",
  allowedAgents: [{ id: "claude", label: "Claude" }],
  debug: "tddy:rpc:*",
};

describe("the startup configuration the daemon hands its web bundle", () => {
  afterEach(() => {
    mock.restore();
  });

  it("comes from the daemon over RPC when the page has no HTTP origin", async () => {
    // Given — a page the desktop application loaded, hosting a daemon in its own process
    const daemon = aDaemonServingItsClientConfig();
    const host = aTauriHostedPage(DaemonConfigService, daemon.transport());

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — the daemon answered it over RPC
    expect(config).toEqual(THE_DAEMONS_CLIENT_CONFIG);
    expect(daemon.callsTo(DaemonConfigService.method.getClientConfig)).toHaveLength(1);
  });

  it("comes from /api/config when the page was served over HTTP", async () => {
    // Given — a page a standalone daemon served over HTTP
    const daemon = anHttpDaemonServingItsClientConfig();
    const host = aBrowserPageServedFrom("https://daemon.example");

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — the endpoint the daemon's web server has always served answered it
    expect(config).toEqual(THE_DAEMONS_CLIENT_CONFIG);
    expect(daemon.fetched()).toEqual(["/api/config"]);
  });

  it("is absent when the daemon's web server serves none", async () => {
    // Given — a page whose daemon has no configuration endpoint
    const daemon = anHttpDaemonServingNoClientConfig();
    const host = aBrowserPageServedFrom("https://daemon.example");

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — no configuration, rather than a fabricated one
    expect(config).toEqual(null);
    expect(daemon.fetched()).toEqual(["/api/config"]);
  });

  it("carries the auth gate's session token when it is asked for over RPC", async () => {
    // Given — a desktop page whose auth provider has installed a token resolver
    const daemon = aDaemonServingItsClientConfig();
    const host = aTauriHostedPage(DaemonConfigService, daemon.transport());
    const transport = createDefaultDaemonTransport(
      undefined,
      { current: () => Promise.resolve("fresh-token") },
      host,
    );

    // When — the bundle reads its startup configuration
    await loadClientConfig(transport, host);

    // Then — the request reached the daemon gated like every other RPC
    expect(
      daemon.callsTo(DaemonConfigService.method.getClientConfig).map((c) => c.sessionToken),
    ).toEqual(["fresh-token"]);
  });
});
