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

/** A daemon holding a complete LiveKit block whose common room the operator switched off. */
function aDaemonWithItsCommonRoomSwitchedOff(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(DaemonConfigService.method.getClientConfig, () =>
    create(GetClientConfigResponseSchema, {
      livekitUrl: "ws://127.0.0.1:7880",
      commonRoom: "tddy-lobby",
      daemonMode: true,
      daemonInstanceId: "udoo",
      livekitEnabled: false,
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

/** The same daemon, answering over the HTTP endpoint its web server has always served. */
function anHttpDaemonWithItsCommonRoomSwitchedOff(): HttpEndpoint {
  const fetched: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL) => {
    fetched.push(String(url));
    return Response.json({
      livekit_url: "ws://127.0.0.1:7880",
      common_room: "tddy-lobby",
      daemon_mode: true,
      daemon_instance_id: "udoo",
      livekit_enabled: false,
    });
  });
  return { fetched: () => fetched };
}

/**
 * A daemon whose macOS jail confines the filesystem, answering over RPC.
 *
 * The desktop deployment: no HTTP origin to fetch `/api/config` from, and — being a daemon hosted
 * by the application itself — no common room to advertise the capability in either.
 */
function aDaemonWhoseJailConfinesTheFilesystem(): InMemoryRpcBackend {
  return anInMemoryRpcBackend().onUnary(DaemonConfigService.method.getClientConfig, () =>
    create(GetClientConfigResponseSchema, {
      daemonMode: true,
      daemonInstanceId: "udoo",
      sandboxedCodebase: { confinesFilesystem: true },
    }),
  );
}

/** The same daemon on Linux, whose cgroups jail shares the host filesystem root, over HTTP. */
function anHttpDaemonWhoseJailSharesTheFilesystemRoot(): HttpEndpoint {
  const fetched: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL) => {
    fetched.push(String(url));
    return Response.json({
      daemon_mode: true,
      daemon_instance_id: "udoo",
      sandboxed_codebase: { confines_filesystem: false },
    });
  });
  return { fetched: () => fetched };
}

/** A daemon whose `/api/config` declares `authFlow` as the sign-in flow it serves. */
function anHttpDaemonDeclaringAuthFlow(authFlow: string): HttpEndpoint {
  const fetched: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation(async (url: RequestInfo | URL) => {
    fetched.push(String(url));
    return Response.json({ daemon_mode: true, daemon_instance_id: "udoo", auth_flow: authFlow });
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

/**
 * The configuration both sources are read into, so one assertion covers both paths. Neither
 * fixture daemon declares an `auth_flow`, which is read as a daemon serving no sign-in.
 */
const THE_DAEMONS_CLIENT_CONFIG = {
  livekitUrl: "ws://127.0.0.1:7880",
  livekitRoom: undefined,
  commonRoom: "tddy-lobby",
  daemonMode: true,
  daemonInstanceId: "udoo",
  allowedAgents: [{ id: "claude", label: "Claude" }],
  debug: "tddy:rpc:*",
  authFlow: "none",
};

/** Read the startup configuration as a browser page served from a daemon would. */
function whenABrowserPageReadsItsClientConfig() {
  const host = aBrowserPageServedFrom("https://daemon.example");
  return loadClientConfig(createDefaultDaemonTransport(undefined, undefined, host), host);
}

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

  it("reads the serving daemon's jail capability from /api/config", async () => {
    // Given — a browser page served by a Linux daemon that joins no common room
    anHttpDaemonWhoseJailSharesTheFilesystemRoot();
    const host = aBrowserPageServedFrom("https://daemon.example");

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — it learns what that daemon's jail confines, with no common room to advertise it in
    expect(config?.sandboxedCodebase).toEqual({ confinesFilesystem: false });
  });

  it("reads the serving daemon's jail capability over RPC when the page has no HTTP origin", async () => {
    // Given — a desktop page, hosting its own daemon whose jail confines the filesystem
    const daemon = aDaemonWhoseJailConfinesTheFilesystem();
    const host = aTauriHostedPage(DaemonConfigService, daemon.transport());

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — the RPC mirror of `/api/config` carries the same capability the endpoint would have
    expect(config?.sandboxedCodebase).toEqual({ confinesFilesystem: true });
  });

  it("leaves the jail capability absent when the daemon describes none", async () => {
    // Given — a daemon that predates the capability, or an OS with no sandbox backend
    anHttpDaemonServingItsClientConfig();
    const host = aBrowserPageServedFrom("https://daemon.example");

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — absent stays absent, never a default the form could mistake for a capability
    expect(config?.sandboxedCodebase).toBeUndefined();
  });

  it("reads a daemon that declares no auth_flow as serving no sign-in over HTTP", async () => {
    // Given — a daemon with no GitHub sign-in configured
    anHttpDaemonServingItsClientConfig();

    // When
    const config = await whenABrowserPageReadsItsClientConfig();

    // Then — absence is its own answer, never read as either flow
    expect(config?.authFlow).toEqual("none");
  });

  it("reads a daemon that declares no auth_flow as serving no sign-in over RPC", async () => {
    // Given — a desktop page whose daemon has no GitHub sign-in configured
    const daemon = aDaemonServingItsClientConfig();
    const host = aTauriHostedPage(DaemonConfigService, daemon.transport());

    // When
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then
    expect(config?.authFlow).toEqual("none");
  });

  it("reads the device flow a daemon declares", async () => {
    // Given
    anHttpDaemonDeclaringAuthFlow("device");

    // When
    const config = await whenABrowserPageReadsItsClientConfig();

    // Then
    expect(config?.authFlow).toEqual("device");
  });

  it("keeps an auth_flow it does not recognise as unrecognised rather than guessing a flow", async () => {
    // Given — a daemon declaring a flow this page has never heard of
    anHttpDaemonDeclaringAuthFlow("carrier-pigeon");

    // When
    const config = await whenABrowserPageReadsItsClientConfig();

    // Then — the value is kept so the sign-in screen can name it
    expect(config?.authFlow).toEqual({ unrecognised: "carrier-pigeon" });
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

  it("tells the page the common room is switched off when it is asked over RPC", async () => {
    // Given — a page whose daemon holds LiveKit credentials it was told not to use
    const daemon = aDaemonWithItsCommonRoomSwitchedOff();
    const host = aTauriHostedPage(DaemonConfigService, daemon.transport());

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — it knows. A url and a room name are still there, so without the flag the page would
    // build a `Room` and mint a token for a room the daemon deliberately is not in.
    expect(config?.livekitEnabled).toEqual(false);
  });

  it("tells the page the common room is switched off when it is served over HTTP", async () => {
    // Given — the same daemon, reached the way a browser dashboard reaches it
    const daemon = anHttpDaemonWithItsCommonRoomSwitchedOff();
    const host = aBrowserPageServedFrom("https://daemon.example");

    // When — the bundle reads its startup configuration
    const config = await loadClientConfig(
      createDefaultDaemonTransport(undefined, undefined, host),
      host,
    );

    // Then — it knows over this route too. The two payloads are mapped by hand in separate
    // branches, so a flag added to one and forgotten in the other is the expected failure.
    expect(config?.livekitEnabled).toEqual(false);
    expect(daemon.fetched()).toEqual(["/api/config"]);
  });
});
