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

/**
 * Resolve `hostId` the way the registry does — first registered provider that claims it.
 *
 * Taken directly rather than through `ConnectionProviderRegistry`, which is node 1's and is still
 * unimplemented on this branch. Precedence is what these specs are about, so it is spelled out here
 * rather than borrowed from an unimplemented dependency: driving through the registry would make
 * every failure read as node 1's.
 */
function resolveThrough(providers: ConnectionProvider[], hostId: string) {
  for (const provider of providers) {
    const connection = provider.connectHost(hostId);
    if (connection) return connection;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

function ResolutionProbe({
  providers,
  hostId,
}: {
  providers: ConnectionProvider[];
  hostId: string;
}) {
  const connection = resolveThrough(providers, hostId);
  return (
    <div>
      <div data-testid="provider">{connection?.providerId ?? "unreachable"}</div>
      <div data-testid="capabilities">
        {connection ? [...connection.capabilities].sort().join(",") : "none"}
      </div>
    </div>
  );
}

function LiveKitDecisionProbe({ config }: { config: { livekitUrl?: string; commonRoom?: string } }) {
  return (
    <div data-testid="livekit">{liveKitIsConfigured(config) ? "brought up" : "not started"}</div>
  );
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("a desktop app with no LiveKit configuration", () => {
  it("reaches its own host over IPC", () => {
    // Given only the desktop's own provider — nothing else is registered because nothing else is
    // configured
    const providers = [createIpcConnectionProvider(aDesktopRegistration())];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Then the app has a working host. Today it has none: the host list is the common room, and
    // the common room was never joined.
    byTestId("provider").should("have.text", "ipc");
  });

  it("advertises rpc only, so the media surfaces do not apply", () => {
    const providers = [createIpcConnectionProvider(aDesktopRegistration())];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Node 4's gating then hides VNC, screen sharing and the participant list. Absent, not broken.
    byTestId("capabilities").should("have.text", "rpc");
  });

  it("starts no LiveKit connection at all", () => {
    cy.mount(<LiveKitDecisionProbe config={{}} />);

    // Then no room is joined, no token is minted, and no `Room` is constructed
    byTestId("livekit").should("have.text", "not started");
  });

  it("sees no peers, and says so rather than failing", () => {
    const providers = [createIpcConnectionProvider(aDesktopRegistration())];

    cy.mount(<ResolutionProbe providers={providers} hostId={A_PEER} />);

    byTestId("provider").should("have.text", "unreachable");
  });
});

describe("a desktop app that is configured for LiveKit", () => {
  it("still reaches its own host over IPC, not through the media server", () => {
    // Given both providers, the desktop's registered first
    const providers = [createIpcConnectionProvider(aDesktopRegistration()), aLiveKitProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Then the local host stays on IPC even though LiveKit could also reach that machine. The
    // daemon is in this process; a round trip out to a media server and back is pure latency.
    // Registration order expresses that — there is no preference setting.
    byTestId("provider").should("have.text", "ipc");
  });

  it("reaches a peer over LiveKit, with everything that carries", () => {
    const providers = [createIpcConnectionProvider(aDesktopRegistration()), aLiveKitProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={A_PEER} />);

    // Then a peer is fully capable — the same host is media-capable over LiveKit and not over IPC,
    // which is exactly why capabilities live on the connection and not on the host
    byTestId("provider").should("have.text", "livekit");
    byTestId("capabilities").should("have.text", "media,presence,rpc");
  });

  it("brings LiveKit up only when both a url and a room are configured", () => {
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
describe("a browser, where the IPC override does not exist", () => {
  it("reaches the desktop machine's host over LiveKit", () => {
    // Given only the LiveKit provider — the browser bundle never loads the desktop's module
    const providers = [aLiveKitProvider()];

    cy.mount(<ResolutionProbe providers={providers} hostId={THIS_HOST} />);

    // Then choosing that machine in a browser works exactly as it does today, with full
    // capabilities. Nothing about the browser path changes.
    byTestId("provider").should("have.text", "livekit");
    byTestId("capabilities").should("have.text", "media,presence,rpc");
  });
});

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

/** Reads the wire a host is actually reached over, through the registry rather than by hand. */
function RegisteredResolutionProbe({ hostId }: { hostId: string }) {
  const connection = useHostConnection(hostId);
  return <div data-testid="provider">{connection?.providerId ?? "unreachable"}</div>;
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
          <RegisteredResolutionProbe hostId={THIS_HOST} />
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
    // page has no local host to register. This is the behavioural half of the guard below, and the
    // one that would actually fail if a later change registered the IPC wire everywhere.
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
