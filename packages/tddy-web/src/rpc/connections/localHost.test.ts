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
import type { Transport } from "@connectrpc/connect";
import {
  createTauriTransport,
  sessionTarget,
  type ConnectionTarget,
  type WebviewIpcBridge,
  type WebviewIpcHost,
} from "tddy-tauri-web";
import { ConnectionService } from "../../gen/connection_pb";
import type { SessionConnection } from "./session";
import type { HostConnection } from "./types";
import {
  createIpcConnectionProvider,
  createLocalHostDirectorySource,
  liveKitIsConfigured,
  localHostRegistrationFor,
  LOCAL_IPC_SOURCE_ID,
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

/** A page the Tauri host application loaded: it injects its IPC internals into every one. */
function aPageInsideTheDesktopApp() {
  return { __TAURI_INTERNALS__: {} };
}

/** A page a browser loaded over HTTP from the daemon that serves the bundle. */
function aPageInABrowser() {
  return {};
}

describe("whether this page has a local host at all", () => {
  it("has one when the host application loaded it", () => {
    const registration = localHostRegistrationFor(aPageInsideTheDesktopApp(), THIS_HOST);

    // The daemon is in this page's own process, so it is reachable before anything else is
    expect(registration?.daemonInstanceId).toBe(THIS_HOST);
  });

  it("names the host the way a daemon names itself in a common room", () => {
    const registration = localHostRegistrationFor(aPageInsideTheDesktopApp(), THIS_HOST);

    // So a machine that both sources describe does not change name in the selector depending on
    // which account of it the directory happened to keep
    expect(registration?.label).toBe(`${THIS_HOST} (this daemon)`);
  });

  it("has none in a browser", () => {
    // Given the same bundle, loaded over HTTP instead — one build serves both hosts
    const registration = localHostRegistrationFor(aPageInABrowser(), THIS_HOST);

    // Then there is nothing to register, which is the whole of how the browser stays on LiveKit:
    // not a second `isDesktop` question, but the one `daemonTransportFlavour` already answers to
    // choose how this page reaches its own daemon
    expect(registration).toBeNull();
  });

  it("has none when the daemon named no instance", () => {
    // A bundle served by something that is not a daemon — a Storybook build — knows no host id
    expect(localHostRegistrationFor(aPageInsideTheDesktopApp(), undefined)).toBeNull();
    expect(localHostRegistrationFor(aPageInsideTheDesktopApp(), "   ")).toBeNull();
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

  it("stamps its own source id on the host it contributes", () => {
    const source = createLocalHostDirectorySource(aRegistration());

    // Which is what a diagnostic reads to say *which account* of a machine the directory kept. It
    // is not how this source wins anything: it is registered behind the common room deliberately,
    // because a room's advertisement carries an attachment cap this one cannot know — see the
    // ordering note in `useDirectorySources`.
    expect(source.hosts[0]?.sourceId).toBe(LOCAL_IPC_SOURCE_ID);
    expect(source.id).toBe(LOCAL_IPC_SOURCE_ID);
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
  /** How many times a transport registered a response channel on this bridge. */
  connectCount(): number;
  /** Whether the page released this connection — the host-side peer a detach must not leak. */
  wasReleased(): boolean;
}

/**
 * Epochs for the doubles below.
 *
 * A bridge owns its epoch and it is fixed for the connection's lifetime, so two bridges must not
 * share one — the host refuses an epoch an open connection already holds.
 */
let nextClientEpoch = 1;

/**
 * One host-application connection, faithful to `tddy-tauri-web`'s bridge in the three ways a
 * lifetime depends on.
 *
 * **A second `connect` is refused**, because the real one invokes `tddy_rpc_connect` with the
 * bridge's own epoch and `multi_host.rs` answers `EpochInUse` for an epoch it already holds. A
 * double that accepted it would let a second transport be built over one bridge and report nothing
 * — which is exactly the shape of bug this file exists to catch.
 *
 * **`closed` resolves on release**, as the real bridge's does, so anything watching for the
 * connection to end sees it end.
 *
 * `onReleased` is how the registry below learns to drop its entry, mirroring the callback
 * `createTauriIpcBridgeTo` is constructed with.
 */
function aBridgeDouble(onReleased: () => void): BridgeDouble {
  let connects = 0;
  let released = false;
  let reportGone: (reason: string) => void = () => {};
  const closed = new Promise<string>((resolve) => {
    reportGone = resolve;
  });
  return {
    clientEpoch: nextClientEpoch++,
    async connect() {
      if (released) return;
      connects += 1;
      if (connects > 1) {
        throw new Error(`the host application refused a second connection under one epoch`);
      }
    },
    send: async () => {},
    closed,
    async close() {
      if (released) return;
      released = true;
      onReleased();
      reportGone("the page released this connection");
    },
    connectCount: () => connects,
    wasReleased: () => released,
  };
}

/**
 * The page's connections to the host application: one bridge per target, dropped on release.
 *
 * The registry deletes an entry as its bridge is released, which the real one does too — so a target
 * reattached after a detach opens a *fresh* connection rather than being handed the released one.
 * `openConnection` is the factory and nothing else inspects through it: an inspector that minted a
 * bridge would change the very thing it was asked about.
 */
interface IpcHostDouble extends WebviewIpcHost {
  /** The targets the page asked for a connection to, in the order it asked. */
  targetsOpened(): string[];
  /** Every bridge ever opened for `target`, oldest first. Mints nothing. */
  bridgesFor(target: ConnectionTarget): BridgeDouble[];
}

function anIpcHostDouble(): IpcHostDouble {
  const open = new Map<string, BridgeDouble>();
  const everOpened = new Map<string, BridgeDouble[]>();
  const opened: string[] = [];
  return {
    openConnection(target: ConnectionTarget): WebviewIpcBridge {
      const key = targetKey(target);
      const alreadyOpen = open.get(key);
      if (alreadyOpen) return alreadyOpen;
      const bridge: BridgeDouble = aBridgeDouble(() => {
        if (open.get(key) === bridge) open.delete(key);
      });
      open.set(key, bridge);
      everOpened.set(key, [...(everOpened.get(key) ?? []), bridge]);
      opened.push(key);
      return bridge;
    },
    targetsOpened: () => [...opened],
    bridgesFor: (target) => [...(everOpened.get(targetKey(target)) ?? [])],
  };
}

/** The transport this page already holds to its own daemon, as far as a lifecycle test looks. */
function thePagesDaemonTransport(): Transport {
  return createTauriTransport({ bridge: aBridgeDouble(() => {}) });
}

/** The local host, reached over `ipc` with the page's own transport stack standing in. */
function theLocalHostOver(ipc: WebviewIpcHost): HostConnection {
  const connection = createIpcConnectionProvider(aRegistration(), {
    ipc,
    hostTransport: thePagesDaemonTransport(),
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

/** The one session bridge this page holds for `sessionId`. */
function theWireFor(ipc: IpcHostDouble, sessionId: string): BridgeDouble {
  const bridges = ipc.bridgesFor(sessionTarget(sessionId));
  if (bridges.length !== 1) {
    throw new Error(`expected one connection to session ${sessionId}, found ${bridges.length}`);
  }
  return bridges[0]!;
}

describe("the local host over the page's own daemon connection", () => {
  it("opens no connection of its own to reach the daemon", () => {
    // Given the local host, used
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);

    host.transport();
    host.clientFor(ConnectionService);

    // Then the host application was never asked for a daemon connection. It already holds one —
    // `daemonTransport.ts` opened it before any screen rendered — and a bridge owns one connection
    // under one epoch, so a second transport over it would re-register that epoch, be refused with
    // `EpochInUse`, and leave every call this host made failing.
    expect(ipc.targetsOpened()).toEqual([]);
  });

  it("refuses to reach the host when it was registered without that transport", () => {
    const host = createIpcConnectionProvider(aRegistration(), {
      ipc: anIpcHostDouble(),
    }).connectHost(THIS_HOST);

    // Building one here instead would mean choosing an interceptor stack, and the only stack
    // available to a plain factory is the empty one — which sends whatever token a request happened
    // to carry. Refusing names what is missing; sending stale credentials would not.
    expect(() => host?.transport()).toThrow("without the page's own daemon transport");
  });
});

describe("sessions attached over IPC", () => {
  it("gives each session a connection addressed to it", () => {
    // Given two sessions attached on the local host
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);

    anAttachedSession(host, "session-one");
    anAttachedSession(host, "session-two");

    // Then each has its own wire — concurrent addressed connections are what node 6 made possible,
    // and what a single-connection bridge could never have expressed. The daemon is not among them:
    // the page's connection to it already exists and is not this provider's to open.
    expect(ipc.targetsOpened()).toEqual(["session:session-one", "session:session-two"]);
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
    expect(theWireFor(ipc, "session-one").wasReleased()).toBe(true);
    expect(theWireFor(ipc, "session-two").wasReleased()).toBe(false);
  });

  it("refuses calls issued after a detach rather than routing them nowhere", () => {
    const ipc = anIpcHostDouble();
    const detached = anAttachedSession(theLocalHostOver(ipc), "session-one");

    detached.close();

    // A call on a detached session has no answer coming, and saying so beats leaving it unsettled
    expect(() => detached.transport()).toThrow("session session-one");
    expect(detached.status).toBe("idle");
  });

  it("reattaching a released session opens a fresh connection", () => {
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);

    anAttachedSession(host, "session-one").close();
    anAttachedSession(host, "session-one");

    // The released bridge is gone from the page's registry and its epoch is spent, so the second
    // attach has to be a new connection rather than a second transport over the departed one
    expect(ipc.bridgesFor(sessionTarget("session-one"))).toHaveLength(2);
    expect(ipc.targetsOpened()).toEqual(["session:session-one", "session:session-one"]);
  });
});

describe("two screens attached to one session", () => {
  it("share the one connection the host application holds for it", () => {
    // Given the same session attached twice — `useSessionAttachment` is per screen, so two screens
    // watching one session is ordinary rather than exotic
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);

    anAttachedSession(host, "session-one");
    anAttachedSession(host, "session-one");

    // Then there is one connection and it was registered once. Two would be two transports over the
    // one bridge the host application keys by target — the second registering an epoch already in
    // use, refused, with every call on it failing.
    expect(ipc.bridgesFor(sessionTarget("session-one"))).toHaveLength(1);
    expect(theWireFor(ipc, "session-one").connectCount()).toBe(1);
  });

  it("keeps the wire while either screen is still attached", () => {
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);
    const firstScreen = anAttachedSession(host, "session-one");
    const secondScreen = anAttachedSession(host, "session-one");

    // When one screen detaches
    firstScreen.close();

    // Then the other is untouched. Releasing on the first detach would take the wire out from under
    // a screen still rendering the session's output.
    expect(theWireFor(ipc, "session-one").wasReleased()).toBe(false);
    expect(secondScreen.status).toBe("connected");
    expect(() => secondScreen.transport()).not.toThrow();
  });

  it("releases it when the last screen detaches", () => {
    const ipc = anIpcHostDouble();
    const host = theLocalHostOver(ipc);
    const firstScreen = anAttachedSession(host, "session-one");
    const secondScreen = anAttachedSession(host, "session-one");

    firstScreen.close();
    secondScreen.close();

    // Nobody is watching, so the host-side peer goes — never releasing would leak one per session
    // for the life of the page
    expect(theWireFor(ipc, "session-one").wasReleased()).toBe(true);
  });
});
