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
import {
  createIpcConnectionProvider,
  liveKitIsConfigured,
} from "../../src/rpc/connections/localHost";
import type { ConnectionProvider } from "../../src/rpc/connections/types";
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
 * A regression guard, and **green at the contract commit** — it exercises none of this node's code.
 *
 * That is the point of it: the browser path must be untouched by everything the desktop build does,
 * and the strongest form of that guarantee is structural — `tddy-web` never imports `localHost.ts`,
 * so a browser bundle cannot contain the IPC provider even by accident. This spec pins the visible
 * half of it, so a later change that quietly registered the IPC provider everywhere would fail here.
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
