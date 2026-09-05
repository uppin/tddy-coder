/**
 * Unit tests for the desktop build's own host registration.
 *
 * Two rules carry this node. **LiveKit is configured or it is not**, and "not" must be a working
 * state rather than a broken one — with either the URL or the common room missing, nothing is
 * joined, no token is minted, and no `Room` is constructed. And **the local host is always there**,
 * from `daemonInstanceId`, which the daemon serves ungated and therefore before sign-in.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host.md`
 */

import { describe, it, expect } from "bun:test";
import {
  createTauriTransport,
  sessionTarget,
  type ConnectionTarget,
  type WebviewIpcBridge,
  type WebviewIpcHost,
} from "tddy-tauri-web";
import type { SessionConnection } from "./session";
import type { HostConnection } from "./types";
import {
  createIpcConnectionProvider,
  createLocalHostDirectorySource,
  liveKitIsConfigured,
} from "./localHost";

const THIS_HOST = "instance-this-host";

function aRegistration() {
  return { daemonInstanceId: THIS_HOST, label: "this daemon" };
}

describe("whether LiveKit should be brought up", () => {
  it("is configured when both a url and a common room are given", () => {
    expect(
      liveKitIsConfigured({ livekitUrl: "wss://livekit.example", commonRoom: "tddy" }),
    ).toBe(true);
  });

  it("is not configured when the common room is missing", () => {
    // A url with no room names a server but nothing to join — there is no host list to build
    expect(liveKitIsConfigured({ livekitUrl: "wss://livekit.example" })).toBe(false);
  });

  it("is not configured when the url is missing", () => {
    expect(liveKitIsConfigured({ commonRoom: "tddy" })).toBe(false);
  });

  it("is not configured when nothing is given at all", () => {
    // The desktop app's default. This must be a *working* state: no room joined, no token minted,
    // no Room constructed, and the app fully usable on its own host.
    expect(liveKitIsConfigured({})).toBe(false);
  });

  it("treats blank and whitespace-only settings as unconfigured", () => {
    // The daemon serves absent fields as empty strings, so a blank must not read as configured —
    // it would produce a join attempt against `""` and a connection error on every screen
    expect(liveKitIsConfigured({ livekitUrl: "", commonRoom: "" })).toBe(false);
    expect(liveKitIsConfigured({ livekitUrl: "   ", commonRoom: "tddy" })).toBe(false);
    expect(liveKitIsConfigured({ livekitUrl: "wss://livekit.example", commonRoom: "  " })).toBe(
      false,
    );
  });
});

describe("the desktop's own directory source", () => {
  it("contributes the serving daemon, and only it", () => {
    // Given the desktop's source
    const source = createLocalHostDirectorySource(aRegistration());

    // Then the app has a host to select before it has reached anything at all — today it has none,
    // because the host list *is* the common room and the desktop app never joins one
    expect(source.hosts.map((host) => host.hostId)).toEqual([THIS_HOST]);
    expect(source.hosts[0]?.label).toBe("this daemon");
  });

  it("is connected, because there is nothing to connect to", () => {
    const source = createLocalHostDirectorySource(aRegistration());

    // The daemon is in this process and it served this page. Reporting anything else would put a
    // "connecting…" or a failure on a screen that is already talking to it.
    expect(source.status).toBe("connected");
    expect(source.error).toBeNull();
  });

  it("stamps its own source id, so the merge prefers it over a common-room advertisement", () => {
    const source = createLocalHostDirectorySource(aRegistration());

    // The directory de-duplicates by host id, first source winning. Registering this one ahead of
    // LiveKit is what keeps the desktop's account of its own machine; the id is how a diagnostic
    // says which account won.
    expect(source.id).toBe(source.hosts[0]?.sourceId);
  });

  it("offers no host when the daemon named no instance", () => {
    // Given a page served by something that is not a daemon — a static file server, a Storybook
    // build — the client config carries no instance id
    const source = createLocalHostDirectorySource({ daemonInstanceId: "", label: "unknown" });

    // Then there is nothing to contribute, and saying so is `idle`: an absent local daemon is not a
    // fault, and a source claiming a host called "" would shadow every other account of it
    expect(source.hosts).toEqual([]);
    expect(source.status).toBe("idle");
    expect(source.error).toBeNull();
  });
});

describe("the desktop's own connection provider", () => {
  it("claims its own host and nothing else", () => {
    // Given the provider
    const provider = createIpcConnectionProvider(aRegistration());

    // Then it reaches its own machine over IPC, and declines every peer — which is what leaves the
    // peers to the LiveKit provider registered behind it
    expect(provider.connectHost(THIS_HOST)).not.toBeNull();
    expect(provider.connectHost("instance-a-peer")).toBeNull();
  });

  it("advertises rpc only, never media or presence", () => {
    // Given a connection to the local host
    const connection = createIpcConnectionProvider(aRegistration()).connectHost(THIS_HOST);

    // Then it is honest about what a frame pipe can carry. Publishing media into a LiveKit room to
    // fill the gap would make the desktop's own host quietly require the thing this stack made
    // optional — the surfaces are absent instead, which is node 4's job.
    expect(connection).not.toBeNull();
    expect([...(connection?.capabilities ?? [])]).toEqual(["rpc"]);
  });
});

// ---------------------------------------------------------------------------
// The host application, as much of it as a connection lifecycle observes
// ---------------------------------------------------------------------------

/** How the host application keys its connections: one bridge per target (`transport.ts`). */
function targetKey(target: ConnectionTarget): string {
  return target.kind === "daemon" ? "daemon" : `session:${target.sessionId}`;
}

interface BridgeDouble extends WebviewIpcBridge {
  /** Whether the page released this connection — the host-side peer a detach must not leak. */
  wasReleased(): boolean;
}

function aBridgeDouble(): BridgeDouble {
  let released = false;
  return {
    connect: async () => {},
    send: async () => {},
    // Never resolves: the host application and the page share one lifetime, so nothing but a
    // release ends a connection, and a release goes through `close`.
    closed: new Promise<string>(() => {}),
    close: async () => {
      released = true;
    },
    wasReleased: () => released,
  };
}

interface IpcHostDouble extends WebviewIpcHost {
  /** The targets the page asked for a connection to, in the order it asked. */
  targetsOpened(): string[];
  bridgeTo(target: ConnectionTarget): BridgeDouble;
}

function anIpcHostDouble(): IpcHostDouble {
  const bridges = new Map<string, BridgeDouble>();
  const opened: string[] = [];
  const bridgeTo = (target: ConnectionTarget): BridgeDouble => {
    const key = targetKey(target);
    const alreadyOpen = bridges.get(key);
    if (alreadyOpen) return alreadyOpen;
    const bridge = aBridgeDouble();
    bridges.set(key, bridge);
    opened.push(key);
    return bridge;
  };
  return { openConnection: bridgeTo, bridgeTo, targetsOpened: () => [...opened] };
}

/** The local host, reached over `ipc` with the page's own transport stack standing in. */
function theLocalHostOver(ipc: WebviewIpcHost): HostConnection {
  const connection = createIpcConnectionProvider(aRegistration(), {
    ipc,
    transportFor: (bridge) => createTauriTransport({ bridge }),
  }).connectHost(THIS_HOST);
  if (!connection) throw new Error(`the IPC provider declined its own host ${THIS_HOST}`);
  return connection;
}

/**
 * `sessionId`, attached and in use.
 *
 * The call is what opens the wire: a connection resolved but never called costs the host
 * application nothing, so a lifecycle assertion has to reach the point a real screen reaches.
 */
function anAttachedSession(host: HostConnection, sessionId: string): SessionConnection {
  const session = host.openSession(sessionId, { sessionId });
  session.transport();
  return session;
}

describe("sessions attached over IPC", () => {
  it("gives each session a connection addressed to it", () => {
    // Given two sessions attached on the local host
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);
    host.transport();

    anAttachedSession(host, "session-one");
    anAttachedSession(host, "session-two");

    // Then each has its own wire, alongside the host's own — concurrent addressed connections are
    // what node 6 made possible, and what a single-connection bridge could never have expressed
    expect(ipc.targetsOpened()).toEqual([
      "daemon",
      "session:session-one",
      "session:session-two",
    ]);
  });

  it("releases only the session that was detached", () => {
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);
    const detached = anAttachedSession(host, "session-one");
    anAttachedSession(host, "session-two");

    // When one session is detached
    detached.close();

    // Then the host drops that peer and no other. Forgetting a connection instead of releasing it
    // would leak a host-side peer per attach; releasing too much would silently kill the sessions
    // the operator is still watching.
    expect(ipc.bridgeTo(sessionTarget("session-one")).wasReleased()).toBe(true);
    expect(ipc.bridgeTo(sessionTarget("session-two")).wasReleased()).toBe(false);
  });

  it("refuses calls issued after a detach rather than routing them nowhere", () => {
    const ipc = anIpcHostDouble();
    const detached = anAttachedSession(theLocalHostOver(ipc), "session-one");

    detached.close();

    // A call on a detached session has no answer coming, and saying so beats leaving it unsettled
    expect(() => detached.transport()).toThrow("session session-one");
    expect(detached.status).toBe("idle");
  });
});
