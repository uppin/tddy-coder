/**
 * Acceptance spec: the host directory works with LiveKit switched off.
 *
 * `SelectedDaemonProvider` used to *be* the host list: it joined a common room and read the daemons
 * off its participants. With no `livekitUrl` / `commonRoom`, `useCommonRoom` short-circuited to
 * `idle`, `daemons` stayed `[]`, and the selector offered **nothing** — not even the daemon serving
 * the page, which `/api/config` has always named as `daemon_instance_id`. That is why a
 * `tddy-desktop` which does not join a common room by default could reach no host at all.
 *
 * These specs pin the replacement: a directory merged from sources, where an unconfigured source
 * contributes nothing and reports `idle` rather than `error`, and the serving daemon is always
 * offered.
 *
 * Technical: `packages/tddy-web/docs/host-directory.md`
 * Stack: `optional-livekit` node 2 of 7.
 */

import React from "react";
import { useHostDirectory } from "../../src/rpc/hostDirectory/useHostDirectory";
import { useHostPresence } from "../../src/rpc/hostDirectory/useHostPresence";
import type { HostDirectorySource } from "../../src/rpc/hostDirectory/types";
import { HostDirectorySources } from "../../src/rpc/hostDirectory/useHostDirectory";
import { DaemonSelectorConnected } from "../../src/components/shell/DaemonSelector";
import { SelectedDaemonProvider } from "../../src/rpc/selectedDaemon";
import { daemonSelectorPage } from "../support/pages/daemonSelectorPage";
import { ConnectionProviders, ConnectionProviderRegistry } from "../../src/rpc/connections/registry";
import type { ConnectionCapability, HostConnection } from "../../src/rpc/connections/types";
import { HostPresenceRoom } from "../../src/rpc/hostDirectory/presenceRoom";
import { aFakeCommonRoom } from "../support/livekit/fakeCommonRoom";
import { byTestId } from "../support/testIds";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const THIS_HOST = "instance-this-host";
const A_PEER = "instance-a-peer";

/** The daemon serving this page, from `/api/config`'s `daemon_instance_id`. */
function aServingSource(): HostDirectorySource {
  return {
    id: "serving",
    status: "connected",
    error: null,
    hosts: [{ hostId: THIS_HOST, label: "this daemon", sourceId: "serving" }],
  };
}

/** A common room that was never configured: no hosts, and idle rather than broken. */
function anUnconfiguredLiveKitSource(): HostDirectorySource {
  return { id: "livekit", status: "idle", error: null, hosts: [] };
}

/** A common room that is up, advertising one peer. */
function aConnectedLiveKitSource(): HostDirectorySource {
  return {
    id: "livekit",
    status: "connected",
    error: null,
    hosts: [{ hostId: A_PEER, label: "laptop-b (this daemon)", sourceId: "livekit" }],
  };
}

/** A common room that cannot be reached. */
function aFailedLiveKitSource(): HostDirectorySource {
  return {
    id: "livekit",
    status: "error",
    error: "could not reach the LiveKit server",
    hosts: [],
  };
}

/**
 * A registry that reaches `THIS_HOST` over a wire advertising exactly `capabilities`.
 *
 * Presence is refused on two independent grounds — no connection at all, or a connection without
 * the capability — and only the second is the seam this PR delivers. A spec that mounts no registry
 * exercises the first and would pass with the capability check deleted, so every presence spec
 * below registers a wire that really does reach the host.
 */
function aWireReachingThisHostWith(
  ...capabilities: ConnectionCapability[]
): ConnectionProviderRegistry {
  const registry = new ConnectionProviderRegistry();
  registry.register({
    id: "in-memory",
    connectHost: (hostId) =>
      hostId === THIS_HOST
        ? ({
            hostId,
            providerId: "in-memory",
            status: "connected",
            error: null,
            capabilities: new Set(capabilities),
            // Presence never issues RPC. Throwing rather than returning a double keeps a spec that
            // starts to depend on the wire loudly broken instead of quietly meaningless.
            clientFor: () => {
              throw new Error("this spec never issues RPC");
            },
            transport: () => {
              throw new Error("this spec never issues RPC");
            },
          } as HostConnection)
        : null,
  });
  return registry;
}

// ---------------------------------------------------------------------------
// Probes
// ---------------------------------------------------------------------------

/** Renders the directory the way a host selector would read it. */
function DirectoryProbe() {
  const directory = useHostDirectory();
  return (
    <div>
      <div data-testid="host-ids">{directory.hosts.map((h) => h.hostId).join(",") || "none"}</div>
      <div data-testid="directory-status">{directory.status}</div>
      <div data-testid="directory-error">{directory.error ?? "no error"}</div>
      <div data-testid="livekit-source-status">
        {directory.sources.find((s) => s.id === "livekit")?.status ?? "absent"}
      </div>
    </div>
  );
}

/** Named readers for the probes above. No raw selector belongs in a test body. */
const hostDirectoryProbe = {
  expectOffersHosts(...hostIds: string[]) {
    byTestId("host-ids").should("have.text", hostIds.length ? hostIds.join(",") : "none");
  },
  expectStatus(status: string) {
    byTestId("directory-status").should("have.text", status);
  },
  expectNoError() {
    byTestId("directory-error").should("have.text", "no error");
  },
  expectLiveKitSourceStatus(status: string) {
    byTestId("livekit-source-status").should("have.text", status);
  },
  expectPresence(availability: "available" | "unavailable") {
    byTestId("presence").should("have.text", availability);
  },
};

/** Asks for a host's participant roster by name, which a presence-less connection refuses. */
function PresenceProbe({ hostId }: { hostId: string }) {
  const presence = useHostPresence(hostId);
  return <div data-testid="presence">{presence ? "available" : "unavailable"}</div>;
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

describe("the host directory with LiveKit unconfigured", () => {
  it("offers the daemon serving the page when no common room is configured", () => {
    // Given a page with no `livekitUrl` and no `commonRoom` — tddy-desktop's default
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), anUnconfiguredLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then the serving daemon is offered and the directory is usable. This list used to be empty
    // and the selector showed nothing at all.
    hostDirectoryProbe.expectOffersHosts(THIS_HOST);
    hostDirectoryProbe.expectStatus("connected");
  });

  it("calls an unconfigured common room idle, never an error", () => {
    // Given the same page
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), anUnconfiguredLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then nothing reports a failure. An operator who deliberately did not configure LiveKit
    // must not be shown a connection error for it on every screen.
    hostDirectoryProbe.expectLiveKitSourceStatus("idle");
    hostDirectoryProbe.expectNoError();
  });

  it("refuses presence on a host reached over a wire that does not advertise it", () => {
    // Given a host that really is reachable — over a wire offering `rpc` and nothing else — and a
    // common room sitting in scope. The room is there deliberately: it makes the missing capability
    // the *only* reason presence could be refused, so this asserts the gate rather than the
    // absence of a connection.
    cy.mount(
      <ConnectionProviders registry={aWireReachingThisHostWith("rpc")}>
        <HostPresenceRoom room={aFakeCommonRoom().room}>
          <PresenceProbe hostId={THIS_HOST} />
        </HostPresenceRoom>
      </ConnectionProviders>,
    );

    // Then a component that wants the participant roster is told it is unavailable, rather than
    // helping itself to the room off a shared context. This is the seam node 4 gates the presence
    // surfaces on.
    hostDirectoryProbe.expectPresence("unavailable");
  });

  it("hands the roster to a host reached over a wire that does advertise presence", () => {
    // Given the same room, reached over a wire that does offer presence
    cy.mount(
      <ConnectionProviders registry={aWireReachingThisHostWith("rpc", "presence")}>
        <HostPresenceRoom room={aFakeCommonRoom().room}>
          <PresenceProbe hostId={THIS_HOST} />
        </HostPresenceRoom>
      </ConnectionProviders>,
    );

    // Then presence is served. Without this the refusal above would also pass with the gate
    // deleted, and nothing would show the capability is read at all.
    hostDirectoryProbe.expectPresence("available");
  });
});

describe("the host directory with LiveKit configured", () => {
  it("shows the serving daemon alongside the common room's peers", () => {
    // Given a page that has both
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), aConnectedLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then both are selectable in the same session — which is what lets a desktop app keep its own
    // host on IPC while reaching its peers over LiveKit
    hostDirectoryProbe.expectOffersHosts(THIS_HOST, A_PEER);
    hostDirectoryProbe.expectStatus("connected");
  });

  it("keeps the local host usable when the common room fails", () => {
    // Given a reachable serving daemon and a common room that will not connect
    cy.mount(
      <HostDirectorySources sources={[aServingSource(), aFailedLiveKitSource()]}>
        <DirectoryProbe />
      </HostDirectorySources>,
    );

    // Then the directory is still connected and the local host is still offered. A LiveKit
    // failure degrades the peers, not the host the operator is sitting in front of.
    hostDirectoryProbe.expectStatus("connected");
    hostDirectoryProbe.expectOffersHosts(THIS_HOST);
    hostDirectoryProbe.expectLiveKitSourceStatus("error");
  });
});

/**
 * The whole daemon-mode selector, driven through the real `SelectedDaemonProvider`.
 *
 * Every spec above supplies `HostDirectorySource` literals, which proves the merge but not that
 * anything *produces* one. These drive the production path — `useServingHostDirectorySource` and
 * `useLiveKitHostDirectorySource` composed by the provider — with no LiveKit configuration at all,
 * which is the configuration this PR exists for and the one nothing else covers.
 */
describe("the daemon selector on a page that was served by a daemon", () => {
  it("offers the serving daemon with no common room configured at all", () => {
    // Given the desktop app's default: a page that knows the daemon that served it and nothing else
    // — no `livekitUrl`, no `commonRoom`, no injected room and no injected host list
    cy.mount(
      <SelectedDaemonProvider servingInstanceId={THIS_HOST}>
        <DaemonSelectorConnected />
      </SelectedDaemonProvider>,
    );

    // Then the serving daemon is offered and already selected, keeping its self-label because it is
    // the daemon serving this page. Before the host directory this selector was empty and disabled.
    daemonSelectorPage.expectShowsSelected(`${THIS_HOST} (this daemon)`);
  });

  it("offers nothing when neither a common room nor a serving daemon names a host", () => {
    // Given a bundle served by something that is not a daemon — a static file server or Storybook —
    // and still no common room
    cy.mount(
      <SelectedDaemonProvider>
        <DaemonSelectorConnected />
      </SelectedDaemonProvider>,
    );

    // Then there is genuinely nothing to offer, and the selector says so rather than naming a host
    // it invented. `idle`, not an error: nothing here failed.
    daemonSelectorPage.expectEmpty();
  });
});
