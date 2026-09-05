/**
 * Acceptance spec: the desktop reaches its own host over IPC, and LiveKit stays optional.
 *
 * This is the node the whole stack exists for. `tddy-desktop` does not join a common room by
 * default, and until now the host list *was* the common room — so it could reach no host at all,
 * not even the daemon running in its own process.
 *
 * Two directions have to hold at once, and they come from one mechanism rather than a mode switch:
 * with no LiveKit configuration the app works entirely on its own host, and with LiveKit configured
 * it reaches peers over LiveKit while keeping its own host on IPC.
 *
 * Changeset: `docs/dev/1-WIP/2026-09-05-optional-livekit-desktop-ipc-host.md`
 * Stack: `optional-livekit` node 7 of 7.
 */

import React from "react";
import { Room } from "livekit-client";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { GenerateTokenResponseSchema, TokenService } from "../../src/gen/token_pb";
import { useLiveKitHostDirectorySource } from "../../src/rpc/hostDirectory/liveKitSource";
import { mountWithRpc } from "../support/rpc/inMemory";
import {
  createIpcConnectionProvider,
  createLocalHostDirectorySource,
  liveKitIsConfigured,
  localHostRegistrationFor,
} from "../../src/rpc/connections/localHost";
import { LocalHostConnections } from "../../src/rpc/connections/localHostRegistration";
import {
  ConnectionProviderRegistry,
  ConnectionProviders,
  useConnectionProviders,
  useHostConnection,
} from "../../src/rpc/connections/registry";
import type { ConnectionProvider } from "../../src/rpc/connections/types";
import { LIVEKIT_SOURCE_ID } from "../../src/rpc/hostDirectory/liveKitSource";
import type { HostDirectorySource } from "../../src/rpc/hostDirectory/types";
import {
  HostDirectorySources,
  useHostDirectory,
} from "../../src/rpc/hostDirectory/useHostDirectory";
import { SelectedDaemonProvider, useDaemons } from "../../src/rpc/selectedDaemon";
import { AuthProvider } from "../../src/hooks/authProvider";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_PEER = "instance-a-peer";

function aDesktopRegistration() {
  return { daemonInstanceId: THIS_HOST, label: "this daemon" };
}

/**
 * The desktop's own connection provider, wired to a host application nothing here should reach.
 *
 * These specs are about which wire claims which host, never about frames, so the IPC host is
 * injected and set to object if it is ever asked. Left to the default, the provider would reach the
 * real page-level `thisPagesIpcHost()` singleton, whose `openConnections` map outlives the mount
 * and cannot be reset from a test — one spec opening a connection would change what the next one
 * sees.
 */
function anIpcProvider(): ConnectionProvider {
  return createIpcConnectionProvider(aDesktopRegistration(), {
    ipc: {
      openConnection: () => {
        throw new Error("no spec here reaches the host application");
      },
    },
  });
}

/** Stands in for the LiveKit provider, which reaches peers and advertises everything. */
function aLiveKitProvider(): ConnectionProvider {
  return {
    id: "livekit",
    connectHost: (hostId) =>
      hostId === A_PEER || hostId === THIS_HOST
        ? {
            hostId,
            providerId: "livekit",
            status: "connected",
            error: null,
            capabilities: new Set(["rpc", "media", "presence"] as const),
            clientFor: () => {
              throw new Error("routing stand-in");
            },
            transport: () => {
              throw new Error("routing stand-in");
            },
            openSession: () => {
              throw new Error("routing stand-in");
            },
          }
        : null,
  };
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/**
 * What wire a host is reached over, and what it can do — read through the real registry.
 *
 * `ConnectionProviderRegistry` resolves a host by asking each registered provider in turn and
 * taking the first that claims it, which is the precedence rule these specs are entirely about. It
 * is driven rather than restated: a local re-implementation of "first match wins" would keep
 * passing if the thing the app actually resolves through stopped obeying it.
 */
function ResolvedHostProbe({ hostId }: { hostId: string }) {
  const connection = useHostConnection(hostId);
  return (
    <div>
      <div data-testid="provider">{connection?.providerId ?? "unreachable"}</div>
      <div data-testid="capabilities">
        {connection ? [...connection.capabilities].sort().join(",") : "none"}
      </div>
    </div>
  );
}

/** Which wire reaches `hostId`, labelled so several hosts can be read from one mount. */
function WireProbe({ hostId, testId }: { hostId: string; testId: string }) {
  const connection = useHostConnection(hostId);
  return <div data-testid={testId}>{connection?.providerId ?? "unreachable"}</div>;
}

/**
 * `providers` registered in order into one registry, with `hostId` resolved through it.
 *
 * Registration order *is* precedence, so the array reads as the deployment it stands for: the
 * desktop's own wire first where the desktop has one, the common room behind it.
 */
function ResolutionProbe({
  providers,
  hostId,
}: {
  providers: ConnectionProvider[];
  hostId: string;
}) {
  const registry = React.useRef<ConnectionProviderRegistry | null>(null);
  registry.current ??= new ConnectionProviderRegistry();
  for (const provider of providers) registry.current.register(provider);
  return (
    <ConnectionProviders registry={registry.current}>
      <ResolvedHostProbe hostId={hostId} />
    </ConnectionProviders>
  );
}

function LiveKitDecisionProbe({ config }: { config: { livekitUrl?: string; commonRoom?: string } }) {
  return (
    <div data-testid="livekit">{liveKitIsConfigured(config) ? "brought up" : "not started"}</div>
  );
}

/** The presence identity a signed-in operator has. Its presence is what makes the spec below bite. */
const A_SIGNED_IN_IDENTITY = "web-someone";

/**
 * The common room actually brought up — or not — from `config`, through the production hook.
 *
 * `roomFactory` is `useCommonRoom`'s own injection seam, so a `Room` constructed on this path is a
 * `Room` the app would have constructed. Reporting the source's status alongside is what
 * distinguishes "did not start" from "started and failed".
 */
function LiveKitStartupProbe({
  config,
  roomFactory,
}: {
  config: { livekitUrl?: string; commonRoom?: string };
  roomFactory: () => Room;
}) {
  const { source } = useLiveKitHostDirectorySource({
    ...config,
    identity: A_SIGNED_IN_IDENTITY,
    roomFactory,
  });
  return <div data-testid="livekit-status">{source.status}</div>;
}

/** A `Room` factory that counts, so "no `Room` is constructed" can be asserted rather than claimed. */
function aRoomFactoryTripwire() {
  let constructed = 0;
  return {
    construct: () => {
      constructed += 1;
      return new Room();
    },
    constructed: () => constructed,
  };
}

/** A daemon that answers a LiveKit token mint, and counts having been asked. */
function aTokenMintTripwire() {
  let minted = 0;
  return {
    backend: anInMemoryRpcBackend().onUnary(TokenService.method.generateToken, () => {
      minted += 1;
      return create(GenerateTokenResponseSchema, { token: "a-token", ttlSeconds: BigInt(3600) });
    }),
    minted: () => minted,
  };
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a desktop app with no LiveKit configuration", () => {
  it("reaches its own host over IPC", () => {
    // Given only the desktop's own provider — nothing else is registered because nothing else is
    // configured
    const providers = [anIpcProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Then the app has a working host. Today it has none: the host list is the common room, and
    // the common room was never joined.
    byTestId("provider").should("have.text", "ipc");
  });

  it("advertises rpc only, so the media surfaces do not apply", () => {
    const providers = [anIpcProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Node 4's gating then hides VNC, screen sharing and the participant list. Absent, not broken.
    byTestId("capabilities").should("have.text", "rpc");
  });

  it("starts no LiveKit connection at all", () => {
    // Given a signed-in operator — so the only thing standing between this page and a join is the
    // configuration it does not have. Without an identity the hook short-circuits anyway and the
    // spec would pass however the configuration were read.
    const rooms = aRoomFactoryTripwire();
    const tokens = aTokenMintTripwire();

    mountWithRpc(<LiveKitStartupProbe config={{}} roomFactory={rooms.construct} />, tokens.backend);

    // Then nothing was started: no token minted, no `Room` constructed. And the source says `idle`
    // rather than `error`, which is the whole of "LiveKit is optional" — an operator who never
    // configured a common room must not be shown a connection failure for it on every screen.
    byTestId("livekit-status").should("have.text", "idle");
    cy.wrap(null).then(() => {
      expect(tokens.minted(), "LiveKit tokens minted").to.equal(0);
      expect(rooms.constructed(), "Room objects constructed").to.equal(0);
    });
  });

  it("sees no peers, and says so rather than failing", () => {
    const providers = [anIpcProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={A_PEER} />);

    byTestId("provider").should("have.text", "unreachable");
  });
});

describe("a desktop app that is configured for LiveKit", () => {
  it("still reaches its own host over IPC, not through the media server", () => {
    // Given both providers, the desktop's registered first
    const providers = [anIpcProvider(), aLiveKitProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Then the local host stays on IPC even though LiveKit could also reach that machine. The
    // daemon is in this process; a round trip out to a media server and back is pure latency.
    // Registration order expresses that — there is no preference setting.
    byTestId("provider").should("have.text", "ipc");
  });

  it("reaches a peer over the common room, which the desktop's own wire declined", () => {
    const providers = [anIpcProvider(), aLiveKitProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={A_PEER} />);

    // The IPC provider claims exactly one host, so a peer falls through to the wire behind it.
    // Only the provider is asserted: what a peer over LiveKit *can do* is the stand-in's answer,
    // not this node's, and asserting it back would be asserting the fixture.
    byTestId("provider").should("have.text", "livekit");
  });

  it("brings LiveKit up when both a url and a room are configured", () => {
    cy.mount(
      <LiveKitDecisionProbe config={{ livekitUrl: "wss://livekit.example", commonRoom: "tddy" }} />,
    );

    byTestId("livekit").should("have.text", "brought up");
  });
});

/**
 * A regression guard: the browser path must be untouched by everything the desktop build does.
 *
 * The guarantee is **behavioural, not structural.** There is one bundle — the Tauri shell loads
 * `packages/tddy-web/dist`, the same files the daemon serves a browser — so `localHost.ts` is in
 * every build there is, and no import graph can keep it out. What keeps a browser off the IPC path
 * is that `localHostRegistrationFor` answers `null` for it and nothing is registered, which
 * *"registers nothing at all for a page a browser loaded"* below drives through the real gate. This
 * spec pins the outcome that matters: choosing that machine in a browser works exactly as it does
 * today, over LiveKit, with full capabilities.
 */


// ---------------------------------------------------------------------------
// Registration: which page gets the IPC wire, and which never does
// ---------------------------------------------------------------------------

/**
 * Stands in for `LiveKitConnections`, which registers the common room from inside
 * `SelectedDaemonProvider` — below the desktop's own registration, which is what orders the two.
 */
function LiveKitStandIn({ children }: { children: React.ReactNode }) {
  const registry = useConnectionProviders();
  const provider = React.useRef<ConnectionProvider | null>(null);
  provider.current ??= aLiveKitProvider();
  registry.register(provider.current);
  return <>{children}</>;
}

/** A page the Tauri host application loaded: it injects its IPC internals into every one. */
function aPageInsideTheDesktopApp() {
  return { __TAURI_INTERNALS__: {} };
}

/** A page a browser loaded over HTTP from the daemon that serves the bundle. */
function aPageInABrowser() {
  return {};
}

/**
 * The app's registration site, as far as this is about: the desktop's own wire offered above the
 * common room, into one registry, for a page that may or may not have a local host.
 */
function mountRegisteredOn(page: { __TAURI_INTERNALS__?: unknown }, registry: ConnectionProviderRegistry) {
  cy.mount(
    <ConnectionProviders registry={registry}>
      <LocalHostConnections registration={localHostRegistrationFor(page, THIS_HOST)}>
        <LiveKitStandIn>
          <ResolvedHostProbe hostId={THIS_HOST} />
        </LiveKitStandIn>
      </LocalHostConnections>
    </ConnectionProviders>,
  );
}

describe("offering the desktop's own wire to the app", () => {
  it("registers it ahead of the common room for a page the host application loaded", () => {
    const registry = new ConnectionProviderRegistry();

    mountRegisteredOn(aPageInsideTheDesktopApp(), registry);

    // Precedence is registration order, and the order is where the components sit — no preference
    // setting, and none wanted. So the local host is reached in-process even though the common room
    // could also reach that machine.
    byTestId("provider").should("have.text", "ipc");
    cy.wrap(null).then(() => expect([...registry.providerIds()]).to.deep.equal(["ipc", "livekit"]));
  });

  it("registers nothing at all for a page a browser loaded", () => {
    const registry = new ConnectionProviderRegistry();

    // Given the very same bundle — there is one build, and it is served to browsers by the daemon
    // and loaded by the desktop shell alike
    mountRegisteredOn(aPageInABrowser(), registry);

    // Then the browser path is untouched: no IPC provider exists to shadow LiveKit, because the
    // page has no local host to register. Choosing that machine in a browser therefore works
    // exactly as it does today, and a later change that registered the IPC wire everywhere would
    // fail right here.
    byTestId("provider").should("have.text", "livekit");
    cy.wrap(null).then(() => expect([...registry.providerIds()]).to.deep.equal(["livekit"]));
  });
});

describe("the desktop's own host in the directory", () => {
  it("names its host where no common room describes it", () => {
    cy.mount(
      <AuthProvider>
        <SelectedDaemonProvider
          room={new Room()}
          daemons={[]}
          hostSources={[createLocalHostDirectorySource(aDesktopRegistration())]}
        >
          <DirectoryProbe />
        </SelectedDaemonProvider>
      </AuthProvider>,
    );

    // With no room and no serving id there was nothing at all before this — the host list *was* the
    // common room, so the desktop app offered the operator no host, not even its own
    byTestId("hosts").should("have.text", THIS_HOST);
  });

  it("does not shadow a common room's richer account of the same machine", () => {
    cy.mount(
      <AuthProvider>
        <SelectedDaemonProvider
          room={new Room()}
          daemons={[{ instanceId: THIS_HOST, label: "advertised", maxAttachmentBytes: 5_000_000 }]}
          hostSources={[createLocalHostDirectorySource(aDesktopRegistration())]}
        >
          <DirectoryProbe />
        </SelectedDaemonProvider>
      </AuthProvider>,
    );

    // The desktop's own account is built from `GetClientConfig`, which carries an instance id and
    // no attachment cap. Letting it win would cost the Start-Session form the client-side refusal
    // for the very host the operator is most likely to use, and gain nothing but a source id.
    byTestId("cap").should("have.text", "5000000");
  });
});

/** The merged directory, as the daemon-mode screens read it. */
function DirectoryProbe() {
  const daemons = useDaemons();
  return (
    <div>
      <div data-testid="hosts">{daemons.map((daemon) => daemon.instanceId).join(",")}</div>
      <div data-testid="cap">{daemons[0]?.maxAttachmentBytes ?? "unadvertised"}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Degradation: LiveKit failing is LiveKit's problem, not the machine's
// ---------------------------------------------------------------------------

/**
 * The common room as it looks once the join has failed: `error`, with a reason worth showing, and
 * no hosts.
 *
 * This is a *source* fixture rather than a driven join because the failure is what the spec is
 * about, not how it came about — `useCommonRoom` has its own coverage for reaching this state, and
 * routing through it here would need a signed-in presence identity and a rejecting `Room` to assert
 * something neither of them is responsible for.
 */
function aFailedLiveKitDirectorySource(): HostDirectorySource {
  return {
    id: LIVEKIT_SOURCE_ID,
    status: "error",
    error: "could not join the common room",
    hosts: [],
  };
}

/**
 * The common room as a *connection provider* once it has failed.
 *
 * It claims no host, which is `LiveKitConnectionProvider`'s own answer whenever it has no room: a
 * join that failed leaves it without one, and saying "I cannot reach that" is a different claim
 * from the host not existing. So the peers become unreachable and nothing else does.
 */
function aLiveKitProviderWithNoRoom(): ConnectionProvider {
  return { id: "livekit", connectHost: () => null };
}

/** The merged directory, as the selector chrome reads it. */
function MergedDirectoryProbe() {
  const directory = useHostDirectory();
  return (
    <div>
      <div data-testid="hosts">{directory.hosts.map((host) => host.hostId).join(",")}</div>
      <div data-testid="directory-status">{directory.status}</div>
      <div data-testid="livekit-error">
        {directory.sources.find((source) => source.id === LIVEKIT_SOURCE_ID)?.error ?? "none"}
      </div>
    </div>
  );
}

/** The directory a desktop app has while its common room is down: one source failed, one working. */
function mountDirectoryWithLiveKitDown() {
  cy.mount(
    <HostDirectorySources
      sources={[
        aFailedLiveKitDirectorySource(),
        createLocalHostDirectorySource(aDesktopRegistration()),
      ]}
    >
      <MergedDirectoryProbe />
    </HostDirectorySources>,
  );
}

describe("a desktop app whose common room has failed", () => {
  it("loses the peers, and only the peers", () => {
    // Given the desktop's own wire, and a common room that cannot reach anything any more. Both
    // hosts are asked in one mount, because the claim is about *which* of them degraded — asking
    // separately could not tell a failed common room from a healthy one.
    const registry = new ConnectionProviderRegistry();
    registry.register(anIpcProvider());
    registry.register(aLiveKitProviderWithNoRoom());

    cy.mount(
      <ConnectionProviders registry={registry}>
        <div>
          <WireProbe hostId={THIS_HOST} testId="local-wire" />
          <WireProbe hostId={A_PEER} testId="peer-wire" />
        </div>
      </ConnectionProviders>,
    );

    // The machine the operator is sitting at is untouched — the daemon is in this process and the
    // media server was never on the path to it. The peer really is out of reach and says so plainly
    // rather than pretending. The degradation is confined to exactly the hosts that needed the wire
    // that failed.
    byTestId("local-wire").should("have.text", "ipc");
    byTestId("peer-wire").should("have.text", "unreachable");
  });

  it("still offers its own host, and does not call the directory broken", () => {
    mountDirectoryWithLiveKitDown();

    // Then the selector still has something to offer, and the chrome above it reads `connected`
    // rather than `error`. One working source is a usable directory — condemning the whole of it
    // for the source that failed is what would put a connection error on a screen that is talking
    // to its host perfectly well.
    byTestId("hosts").should("have.text", THIS_HOST);
    byTestId("directory-status").should("have.text", "connected");
  });

  it("still reports the common room's own failure", () => {
    mountDirectoryWithLiveKitDown();

    // Degrading quietly would be its own bug: an operator who configured a common room and cannot
    // see their fleet has to be told why. The failure belongs to that source and is read off it,
    // which is exactly what keeps it off the directory as a whole.
    byTestId("livekit-error").should("have.text", "could not join the common room");
  });
});

// ---------------------------------------------------------------------------
// Both wires at once — the configuration this stack exists to make possible
// ---------------------------------------------------------------------------

/** A common room that is up, and advertising one peer. */
function aLiveKitDirectorySourceNaming(hostId: string): HostDirectorySource {
  return {
    id: LIVEKIT_SOURCE_ID,
    status: "connected",
    error: null,
    hosts: [{ hostId, label: "a peer", sourceId: LIVEKIT_SOURCE_ID }],
  };
}

describe("a desktop app with LiveKit configured and working", () => {
  it("has its own host and a common-room peer, each over its own wire, in one session", () => {
    // Given the app as it is actually assembled: the desktop's wire ahead of the common room, and
    // the directory merging what each of them knows
    const registry = new ConnectionProviderRegistry();
    registry.register(anIpcProvider());
    registry.register(aLiveKitProvider());

    cy.mount(
      <ConnectionProviders registry={registry}>
        <HostDirectorySources
          sources={[
            aLiveKitDirectorySourceNaming(A_PEER),
            createLocalHostDirectorySource(aDesktopRegistration()),
          ]}
        >
          <div>
            <MergedDirectoryProbe />
            <WireProbe hostId={THIS_HOST} testId="local-wire" />
            <WireProbe hostId={A_PEER} testId="peer-wire" />
          </div>
        </HostDirectorySources>
      </ConnectionProviders>,
    );

    // Then both machines are offered and both are usable — one mount, no reload, no mode switch.
    // Neither is reached the way the other is: the local host stays in-process even though the
    // common room is up and could also reach that machine, and the peer is only reachable because
    // it is up. That is the whole shape of the feature, in one assertion.
    byTestId("hosts").should("have.text", `${A_PEER},${THIS_HOST}`);
    byTestId("local-wire").should("have.text", "ipc");
    byTestId("peer-wire").should("have.text", "livekit");
  });
});
